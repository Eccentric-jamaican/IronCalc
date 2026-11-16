use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
use std::sync::{Arc, OnceLock};

use crate::{
    calc_result::CalcResult, evaluation::LookupKey, expressions::types::CellReferenceIndex,
    model::Model,
};

use super::util::compare_values;
use crate::number_format::to_excel_precision;

static EMPTY_CELL: CalcResult = CalcResult::EmptyCell;

/// The row or column a lookup function searches. A range that runs to the end
/// of the sheet, a whole column or a whole row, is read only as far as the
/// sheet's used area (`Model::clip_to_used_area`): the cells past it are empty
/// and are not read one by one. They still take part in the search. `len` is
/// the length of the whole vector and `get` gives `EmptyCell` past what was
/// read, so a search finds what it would have found had every cell been read.
#[derive(Clone)]
pub(crate) struct LookupVector {
    values: Arc<SearchValues>,
    len: usize,
}

impl From<Vec<CalcResult>> for LookupVector {
    fn from(values: Vec<CalcResult>) -> Self {
        let len = values.len();
        LookupVector {
            values: Arc::new(values.into()),
            len,
        }
    }
}

impl LookupVector {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn get(&self, index: usize) -> &CalcResult {
        self.values.as_slice().get(index).unwrap_or(&EMPTY_CELL)
    }

    pub(crate) fn filled_len(&self) -> usize {
        self.values.filled_len()
    }

    /// The first position whose value satisfies `predicate`. The cells that
    /// were not read are all empty: the first of them is the answer if no
    /// cell read is and an empty cell is.
    pub(crate) fn position(&self, predicate: impl Fn(&CalcResult) -> bool) -> Option<usize> {
        let values = self.values.as_slice();
        if let Some(index) = values.iter().position(&predicate) {
            return Some(index);
        }
        if values.len() < self.len && predicate(&EMPTY_CELL) {
            return Some(values.len());
        }
        None
    }

    /// `SearchValues::exact_position`. A value it finds is never empty, so
    /// the cells that were not read take no part.
    pub(crate) fn exact_position(
        &self,
        equality: Equality,
        target: &CalcResult,
    ) -> Option<Option<usize>> {
        self.values.exact_position(equality, target, false)
    }
}

/// A way a lookup function tells that a value equals the one it looks for,
/// for the values where a hash of the value tells the same.
#[derive(Clone, Copy)]
pub(crate) enum Equality {
    /// `result_matches_regex` with the pattern of a text with no wildcard in
    /// it: the same text, ignoring case as `to_lowercase` does.
    LowercaseText,
    /// `compare_values` or `values_are_equal` on texts: the same text,
    /// ignoring case as `to_uppercase` does. The empty text equals an empty
    /// cell, so it is left out.
    UppercaseText,
    /// `compare_values` on numbers: equal to 15 significant digits.
    RoundedNumber,
    /// `values_are_equal` on numbers.
    Number,
}

impl Equality {
    const COUNT: usize = 4;

    /// The equality of `compare_values(value, target) == 0`.
    pub(crate) fn compared(target: &CalcResult) -> Equality {
        match target {
            CalcResult::String(_) => Equality::UppercaseText,
            _ => Equality::RoundedNumber,
        }
    }

    /// The key of `value` under this equality: two values with a key are
    /// equal if and only if their keys are. A number below 2 in size has
    /// none, since two different numbers there can be closer than
    /// `f64::EPSILON`, which the comparisons take as equal.
    fn key(self, value: &CalcResult) -> Option<ExactKey> {
        match (self, value) {
            (Equality::LowercaseText, CalcResult::String(text))
                if !text.contains(['*', '?', '~']) =>
            {
                Some(ExactKey::Text(text.to_lowercase()))
            }
            (Equality::UppercaseText, CalcResult::String(text)) if !text.is_empty() => {
                Some(ExactKey::Text(text.to_uppercase()))
            }
            (Equality::RoundedNumber, CalcResult::Number(number)) => {
                number_key(to_excel_precision(*number, 15))
            }
            (Equality::Number, CalcResult::Number(number)) => number_key(*number),
            _ => None,
        }
    }
}

fn number_key(number: f64) -> Option<ExactKey> {
    (number.is_finite() && number.abs() >= 2.0).then(|| ExactKey::Number(number.to_bits()))
}

#[derive(PartialEq, Eq, Hash)]
enum ExactKey {
    Text(String),
    Number(u64),
}

/// The first and last positions of a key.
struct Positions {
    first: usize,
    last: usize,
}

/// The values a lookup function searches, with what makes searching them
/// again quick. Within a pass, every lookup over the same range gets the
/// values the first one read (`Model::shared_array`). From the second exact
/// search on, a value is found through an index of the values by key
/// (`Equality::key`), built the first time it is needed, rather than by
/// comparing it with every value in turn.
pub(crate) struct SearchValues {
    values: Vec<CalcResult>,
    exact_searches: AtomicU32,
    indexes: [OnceLock<HashMap<ExactKey, Positions>>; Equality::COUNT],
    filled_len: OnceLock<usize>,
}

impl From<Vec<CalcResult>> for SearchValues {
    fn from(values: Vec<CalcResult>) -> Self {
        SearchValues {
            values,
            exact_searches: AtomicU32::new(0),
            indexes: Default::default(),
            filled_len: OnceLock::new(),
        }
    }
}

impl SearchValues {
    pub(crate) fn as_slice(&self) -> &[CalcResult] {
        &self.values
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    fn filled_len(&self) -> usize {
        *self.filled_len.get_or_init(|| {
            self.values
                .iter()
                .rposition(|value| !matches!(value, CalcResult::EmptyCell))
                .map_or(0, |last| last + 1)
        })
    }

    /// The first position, or the last with `from_end`, of a value equal to
    /// `target` under `equality`, as a search comparing it with every value
    /// would find it. `None` when the index is not used: for the first
    /// search, or when `target` has no key. The caller searches then.
    pub(crate) fn exact_position(
        &self,
        equality: Equality,
        target: &CalcResult,
        from_end: bool,
    ) -> Option<Option<usize>> {
        let key = equality.key(target)?;
        if self.exact_searches.fetch_add(1, AtomicOrdering::Relaxed) == 0 {
            return None;
        }
        let index = self.indexes[equality as usize].get_or_init(|| {
            let mut index: HashMap<ExactKey, Positions> = HashMap::new();
            for (position, value) in self.values.iter().enumerate() {
                if let Some(key) = equality.key(value) {
                    index
                        .entry(key)
                        .and_modify(|positions| positions.last = position)
                        .or_insert(Positions {
                            first: position,
                            last: position,
                        });
                }
            }
            index
        });
        Some(index.get(&key).map(|positions| {
            if from_end {
                positions.last
            } else {
                positions.first
            }
        }))
    }
}

// NOTE: We don't know how Excel exactly implements binary search internally.
// This means that if the values on the lookup range are not in order our results and Excel's will differ

// Assumes values are in ascending order, returns matching index or the largest value smaller than target.
// Returns None if target is smaller than the smaller value.
pub(crate) fn binary_search_or_smaller<T: Ord>(target: &T, array: &[T]) -> Option<i32> {
    // We apply binary search leftmost for value in the range
    let n = array.len();
    let mut l = 0;
    let mut r = n;
    while l < r {
        let m = (l + r) / 2;
        if &array[m] < target {
            l = m + 1;
        } else {
            r = m;
        }
    }
    if l == n {
        return Some((l - 1) as i32);
    }
    // Now l points to the leftmost element
    if &array[l] == target {
        return Some(l as i32);
    }
    // If target is less than the minimum return None
    if l == 0 {
        return None;
    }
    Some((l - 1) as i32)
}

// Assumes values are in ascending order, returns matching index or the smaller value larger than target.
// Returns None if target is smaller than the smaller value.
pub(crate) fn binary_search_or_greater<T: Ord>(target: &T, array: &[T]) -> Option<i32> {
    let mut l = 0;
    let mut r = array.len();
    while l < r {
        let mut m = (l + r) / 2;
        match &array[m].cmp(target) {
            Ordering::Less => {
                l = m + 1;
            }
            Ordering::Greater => {
                r = m;
            }
            Ordering::Equal => {
                while m > 1 {
                    if &array[m - 1] == target {
                        m -= 1;
                    } else {
                        break;
                    }
                }
                return Some(m as i32);
            }
        }
    }
    // If target is larger than the maximum return None
    if r == array.len() {
        return None;
    }
    // Now r points to the rightmost element
    Some(r as i32)
}

// Assumes values are in descending order
pub(crate) fn binary_search_descending_or_smaller<T: Ord>(target: &T, array: &[T]) -> Option<i32> {
    let n = array.len();
    let mut l = 0;
    let mut r = n;
    while l < r {
        let m = (l + r) / 2;
        let mut index = n - m - 1;
        match &array[index].cmp(target) {
            Ordering::Less => {
                l = m + 1;
            }
            Ordering::Greater => {
                r = m;
            }
            Ordering::Equal => {
                while index < n - 1 {
                    if &array[index + 1] == target {
                        index += 1;
                    } else {
                        break;
                    }
                }
                return Some(index as i32);
            }
        }
    }
    if l == 0 {
        return None;
    }
    Some((n - l) as i32)
}

// Assumes values are in descending order, returns matching index or the smaller value larger than target.
// Returns None if target is smaller than the smaller value.
pub(crate) fn binary_search_descending_or_greater<T: Ord>(target: &T, array: &[T]) -> Option<i32> {
    let n = array.len();
    let mut l = 0;
    let mut r = n;
    while l < r {
        let m = (l + r) / 2;
        let mut index = n - m - 1;
        match &array[index].cmp(target) {
            Ordering::Less => {
                l = m + 1;
            }
            Ordering::Greater => {
                r = m;
            }
            Ordering::Equal => {
                while index < n - 1 {
                    if &array[index + 1] == target {
                        index += 1;
                    } else {
                        break;
                    }
                }
                return Some(index as i32);
            }
        }
    }
    if r == n {
        return None;
    }
    Some((n - r - 1) as i32)
}

impl<'a> Model<'a> {
    /// Returns an array with the list of cell values in the range
    pub(crate) fn prepare_array(
        &mut self,
        left: &CellReferenceIndex,
        right: &CellReferenceIndex,
        is_row_vector: bool,
    ) -> Vec<CalcResult> {
        let n = if is_row_vector {
            right.row - left.row
        } else {
            right.column - left.column
        } + 1;
        let mut result = vec![];
        for index in 0..n {
            let row;
            let column;
            if is_row_vector {
                row = left.row + index;
                column = left.column;
            } else {
                column = left.column + index;
                row = left.row;
            }
            let value = self.evaluate_cell(CellReferenceIndex {
                sheet: left.sheet,
                row,
                column,
            });
            result.push(value);
        }
        result
    }

    /// The vector from `left` to `right` for a lookup function, read only as
    /// far as the sheet's used area (see `LookupVector`). `is_row_vector` is
    /// as for `prepare_array`.
    pub(crate) fn lookup_vector(
        &mut self,
        left: &CellReferenceIndex,
        right: &CellReferenceIndex,
        is_row_vector: bool,
    ) -> Result<LookupVector, String> {
        let len = if is_row_vector {
            right.row - left.row
        } else {
            right.column - left.column
        } + 1;
        let (row, column) =
            self.clip_to_used_area(left.sheet, left.row, left.column, right.row, right.column)?;
        let right = CellReferenceIndex {
            sheet: left.sheet,
            row,
            column,
        };
        Ok(LookupVector {
            values: self.shared_array(left, &right, is_row_vector),
            len: len as usize,
        })
    }

    /// `prepare_array` for a function that searches the vector, read once in
    /// a pass and shared by every search of the same vector after it
    /// (`Evaluation::lookup_vectors`), so that a column searched by many
    /// formulas is not read again for each of them. A cell keeps its value for
    /// the rest of a pass once it has been read: a spill that would change a
    /// cell some formula has read restarts the pass, and a restart starts with
    /// no vectors kept. A whole column or row must have been clipped to the
    /// used area already, so that what the vector skips is on record for every
    /// reader (`clip_to_used_area`).
    pub(crate) fn shared_array(
        &mut self,
        left: &CellReferenceIndex,
        right: &CellReferenceIndex,
        is_row_vector: bool,
    ) -> Arc<SearchValues> {
        let key = LookupKey {
            sheet: left.sheet,
            row1: left.row,
            column1: left.column,
            row2: right.row,
            column2: right.column,
            is_row_vector,
        };
        if !self.evaluation.in_pass {
            return Arc::new(self.prepare_array(left, right, is_row_vector).into());
        }
        if let Some(values) = self.evaluation.lookup_vectors.get(&key) {
            return values.clone();
        }
        let read_while_running = self.evaluation.stack.iter().any(|cell| {
            cell.sheet == key.sheet
                && (key.row1..=key.row2).contains(&cell.row)
                && (key.column1..=key.column2).contains(&cell.column)
        });
        let values = Arc::new(SearchValues::from(self.prepare_array(
            left,
            right,
            is_row_vector,
        )));
        self.evaluation.lookup_vectors_read += 1;
        let complete = !self.evaluation.unwinding && self.evaluation.restart.is_none();
        if complete && !read_while_running {
            self.evaluation.keep_lookup_vector(key, &values);
        }
        values
    }
}

/// Old style binary search over a vector of values.
/// Returns the index of the matching element or the largest element smaller
/// than `target`. Returns `-2` if `target` is smaller than every element.
pub(crate) fn binary_search_on_array(target: &CalcResult, array: &LookupVector) -> i32 {
    // We apply binary search leftmost for value in the array
    let mut l = 0;
    let mut r = array.filled_len();
    while l < r {
        let m = (l + r) / 2;
        match compare_values(array.get(m), target) {
            -1 => {
                l = m + 1;
            }
            1 => {
                r = m;
            }
            _ => {
                return m as i32;
            }
        }
    }
    // If target is less than the minimum return #N/A
    if l == 0 {
        return -2;
    }
    // Now l points to the leftmost element
    (l - 1) as i32
}
