#![allow(clippy::unwrap_used)]

use crate::expressions::types::CellReferenceIndex;
use crate::model::Model;
use crate::test::util::new_empty_model;

fn at(model: &Model, cell: &str) -> CellReferenceIndex {
    model._parse_reference(cell)
}

fn ran(model: &mut Model, changed: &[&str]) -> Option<Vec<String>> {
    let cells: Vec<CellReferenceIndex> = changed.iter().map(|cell| at(model, cell)).collect();
    model.evaluate_changed(&cells).map(|roots| {
        roots
            .iter()
            .map(|cell| {
                let name = crate::expressions::utils::number_to_column(cell.column).unwrap();
                format!("{name}{}", cell.row)
            })
            .collect()
    })
}

#[test]
fn only_the_formulas_that_read_a_changed_cell_run_again() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "=A1*2");
    model._set("C1", "=B1+1");
    model._set("D1", "=5*2");
    model._set("E1", "=SUM(A:A)");
    model._set("F1", "=D1+1");
    model.evaluate();
    model._set("A1", "10");
    assert_eq!(
        ran(&mut model, &["A1"]),
        Some(vec!["B1".to_owned(), "C1".to_owned(), "E1".to_owned()])
    );
    assert_eq!(model._get_text("B1"), "20");
    assert_eq!(model._get_text("C1"), "21");
    assert_eq!(model._get_text("E1"), "10");
    assert_eq!(model._get_text("F1"), "11");
}

#[test]
fn a_cell_typed_into_an_empty_part_of_a_range_reaches_its_readers() {
    let mut model = new_empty_model();
    model._set("B1", "=SUM(A1:A100000)");
    model._set("C1", "=COUNTA(A:A)");
    model._set("D1", "=A50000");
    model.evaluate();
    model._set("A50000", "7");
    assert_eq!(
        ran(&mut model, &["A50000"]),
        Some(vec!["B1".to_owned(), "C1".to_owned(), "D1".to_owned()])
    );
    assert_eq!(model._get_text("B1"), "7");
    assert_eq!(model._get_text("C1"), "1");
    assert_eq!(model._get_text("D1"), "7");
}

#[test]
fn a_reference_built_from_data_is_followed() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A2", "2");
    model._set("C1", "1");
    model._set("B1", "=INDIRECT(\"A\"&C1)");
    model._set("B2", "=SUM(OFFSET(A1,C1,0))");
    model.evaluate();
    assert_eq!(model._get_text("B1"), "1");
    model._set("C1", "2");
    ran(&mut model, &["C1"]);
    assert_eq!(model._get_text("B1"), "2");
    assert_eq!(model._get_text("B2"), "0");
    model._set("A2", "20");
    assert_eq!(ran(&mut model, &["A2"]), Some(vec!["B1".to_owned()]));
    assert_eq!(model._get_text("B1"), "20");
    model._set("A3", "30");
    assert_eq!(ran(&mut model, &["A3"]), Some(vec!["B2".to_owned()]));
    assert_eq!(model._get_text("B2"), "30");
}

#[test]
fn only_the_branch_that_ran_is_a_dependency() {
    let mut model = new_empty_model();
    model._set("A1", "TRUE");
    model._set("A2", "5");
    model._set("A3", "6");
    model._set("B1", "=IF(A1,A2,A3)");
    model.evaluate();
    model._set("A3", "60");
    assert_eq!(ran(&mut model, &["A3"]), Some(vec![]));
    model._set("A1", "FALSE");
    ran(&mut model, &["A1"]);
    assert_eq!(model._get_text("B1"), "60");
    model._set("A3", "61");
    assert_eq!(ran(&mut model, &["A3"]), Some(vec!["B1".to_owned()]));
    assert_eq!(model._get_text("B1"), "61");
}

#[test]
fn a_volatile_formula_runs_on_every_change() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "=RAND()");
    model._set("C1", "=B1*0+1");
    model.evaluate();
    model._set("A1", "2");
    assert_eq!(
        ran(&mut model, &["A1"]),
        Some(vec!["B1".to_owned(), "C1".to_owned()])
    );
}

#[test]
fn moving_cells_or_arrays_evaluate_everything() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "=A1+1");
    model.evaluate();
    model.insert_rows(0, 1, 1).unwrap();
    model._set("A2", "5");
    assert_eq!(ran(&mut model, &["A2"]), None);
    assert_eq!(model._get_text("B2"), "6");
    model._set("A2", "6");
    assert_eq!(ran(&mut model, &["A2"]), Some(vec!["B2".to_owned()]));
    model._set("C1", "=SEQUENCE(3)");
    assert_eq!(ran(&mut model, &["C1"]), None);
    assert_eq!(model._get_text("C3"), "3");
    model._set("C1", "4");
    assert_eq!(ran(&mut model, &["C1"]), None);
    assert_eq!(model._get_text("C3"), "");
}

#[test]
fn a_loop_is_left_to_a_full_evaluation() {
    let mut model = new_empty_model();
    model._set("A1", "=B1+1");
    model._set("B1", "1");
    model._set("C1", "=A1*2");
    model.evaluate();
    model._set("B1", "=A1");
    assert_eq!(ran(&mut model, &["B1"]), None);
    assert_eq!(model._get_text("A1"), "#CIRC!");
    assert_eq!(model._get_text("B1"), "#CIRC!");
    model._set("B1", "3");
    assert_eq!(ran(&mut model, &["B1"]), None);
    assert_eq!(model._get_text("A1"), "4");
    assert_eq!(model._get_text("C1"), "8");
    model._set("B1", "4");
    assert_eq!(
        ran(&mut model, &["B1"]),
        Some(vec!["A1".to_owned(), "C1".to_owned()])
    );
    assert_eq!(model._get_text("C1"), "10");
}

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const ROWS: i32 = 6;
const COLUMNS: i32 = 5;
const SHEETS: [&str; 2] = ["Sheet1", "Sheet2"];

fn cell_name(random: &mut Random) -> String {
    let column = (b'A' + random.below(COLUMNS as usize) as u8) as char;
    format!("{column}{}", 1 + random.below(ROWS as usize))
}

fn reference(random: &mut Random) -> String {
    if random.below(5) == 0 {
        format!("{}!{}", random.pick(&SHEETS), cell_name(random))
    } else {
        cell_name(random)
    }
}

fn range(random: &mut Random) -> String {
    let column = (b'A' + random.below(COLUMNS as usize) as u8) as char;
    let top = 1 + random.below(ROWS as usize);
    let bottom = top + random.below(ROWS as usize);
    match random.below(4) {
        0 => format!("{column}:{column}"),
        1 => format!("{column}{top}:{}{bottom}", (column as u8 + 1) as char),
        _ => format!("{column}{top}:{column}{bottom}"),
    }
}

fn bounded(random: &mut Random) -> String {
    let column = (b'A' + random.below(COLUMNS as usize) as u8) as char;
    let top = 1 + random.below(ROWS as usize);
    format!(
        "{column}{top}:{column}{}",
        top + random.below(ROWS as usize)
    )
}

fn input(random: &mut Random, arrays: bool) -> String {
    let r = reference(random);
    let s = reference(random);
    let a = range(random);
    let b = bounded(random);
    let n = 1 + random.below(3);
    match random.below(24) {
        0..=3 => format!("{}", random.below(10)),
        4 => random.pick(&["x", "TRUE", "abc", "-1.5"]).to_owned(),
        5 => String::new(),
        6 => format!("={r}+{s}"),
        7 => format!("=SUM({a})"),
        8 => format!("=IF({r}>3,{s},{n})"),
        9 => format!("=VLOOKUP({r},{a},1,FALSE)"),
        10 => format!("=INDIRECT(\"{}\")", cell_name(random)),
        11 => format!("=SUM(OFFSET({r},{n},0,2,1))"),
        12 => format!("=COUNTIF({a},\">2\")"),
        13 => format!("=IFERROR(1/{r},\"none\")"),
        14 => format!("=MATCH({r},{a},0)"),
        15 => format!("={r}&\"-\"&{s}"),
        16 => format!("=INDEX({a},{n})"),
        17 => format!("=XLOOKUP({r},{a},{a},\"none\")"),
        18 => format!("=COUNTBLANK({a})"),
        19 => format!("=SUMPRODUCT({b},{b})"),
        20 => format!("=MAX({a})-MIN({a})"),
        21 => format!("=VLOOKUP({r},{a},1)"),
        22 if arrays => format!("=SEQUENCE({n})"),
        _ => format!("={r}"),
    }
}

fn everything(model: &Model) -> Vec<String> {
    let mut texts = Vec::new();
    for sheet in 0..SHEETS.len() as u32 {
        for row in 1..=ROWS + 6 {
            for column in 1..=COLUMNS + 1 {
                texts.push(model._get_text_at(sheet, row, column));
            }
        }
    }
    texts
}

#[test]
fn evaluating_changed_cells_gives_what_evaluating_everything_gives() {
    let mut incremental_runs = 0;
    let seeds: u64 = std::env::var("IRONCALC_FUZZ_SEEDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(300);
    for seed in 1..=seeds {
        let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let arrays = seed % 4 == 0;
        let mut changed_only = new_empty_model();
        let mut everything_model = new_empty_model();
        for model in [&mut changed_only, &mut everything_model] {
            model.new_sheet();
        }
        let mut inputs: std::collections::BTreeMap<(u32, i32, i32), String> =
            std::collections::BTreeMap::new();
        let mut cells = Vec::new();
        for _ in 0..20 {
            let sheet = random.below(SHEETS.len()) as u32;
            let row = 1 + random.below(ROWS as usize) as i32;
            let column = 1 + random.below(COLUMNS as usize) as i32;
            cells.push((sheet, row, column, input(&mut random, arrays)));
        }
        for (sheet, row, column, value) in &cells {
            inputs.insert((*sheet, *row, *column), value.clone());
            changed_only
                .set_user_input(*sheet, *row, *column, value.clone())
                .unwrap();
            everything_model
                .set_user_input(*sheet, *row, *column, value.clone())
                .unwrap();
        }
        changed_only.evaluate();
        everything_model.evaluate();
        let mut history: Vec<String> = Vec::new();
        for step in 0..25 {
            let mut changed = Vec::new();
            for _ in 0..1 + random.below(2) {
                let sheet = random.below(SHEETS.len()) as u32;
                let row = 1 + random.below(ROWS as usize) as i32;
                let column = 1 + random.below(COLUMNS as usize) as i32;
                let value = input(&mut random, arrays);
                changed_only
                    .set_user_input(sheet, row, column, value.clone())
                    .unwrap();
                everything_model
                    .set_user_input(sheet, row, column, value.clone())
                    .unwrap();
                inputs.insert((sheet, row, column), value.clone());
                changed.push((CellReferenceIndex { sheet, row, column }, value));
            }
            let positions: Vec<CellReferenceIndex> =
                changed.iter().map(|(cell, _)| *cell).collect();
            let partial = changed_only.evaluate_changed(&positions);
            if partial.is_some() {
                incremental_runs += 1;
            }
            history.push(format!("step {step}: {changed:?} ran {partial:?}"));
            everything_model.evaluate();
            let found = everything(&changed_only);
            let expected = everything(&everything_model);
            if found != expected {
                let mut fresh = new_empty_model();
                fresh.new_sheet();
                for ((sheet, row, column), value) in &inputs {
                    fresh
                        .set_user_input(*sheet, *row, *column, value.clone())
                        .unwrap();
                }
                fresh.evaluate();
                let from_scratch = everything(&fresh);
                let per_sheet = ((ROWS + 6) * (COLUMNS + 1)) as usize;
                let differences: Vec<String> = found
                    .iter()
                    .zip(&expected)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(at, (a, b))| {
                        let c = &from_scratch[at];
                        let sheet = (at / per_sheet) as u32;
                        let row = ((at % per_sheet) / (COLUMNS + 1) as usize + 1) as i32;
                        let column = ((at % per_sheet) % (COLUMNS + 1) as usize + 1) as i32;
                        let formula = changed_only
                            .get_cell_formula(sheet, row, column)
                            .unwrap_or_default()
                            .unwrap_or_default();
                        format!("sheet {sheet} R{row}C{column} {formula:?}: changed-only {a:?}, everything {b:?}, from scratch {c:?}")
                    })
                    .collect();
                panic!(
                    "seed {seed}, step {step}, after {changed:?}\n{}\ncells {cells:?}\n{}",
                    differences.join("\n"),
                    history.join("\n")
                );
            }
        }
    }
    assert!(
        incremental_runs > seeds as usize,
        "only {incremental_runs} edits were evaluated incrementally"
    );
}
