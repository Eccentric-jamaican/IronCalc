use crate::test::util::new_empty_model;

const TARGETS: [&str; 7] = ["0", "1", "3", "3.5", "5", "99", "-99"];

const TEXT_TARGETS: [&str; 5] = ["\"a\"", "\"banana\"", "\"c\"", "\"fig\"", "\"zzz\""];

const DESCENDING_TARGETS: [&str; 6] = ["99", "50", "35", "10", "5", "-99"];

const PAIRS: [(&str, &str); 9] = [
    ("=VLOOKUP({t},A:B,2,TRUE)", "=VLOOKUP({t},A1:B5,2,TRUE)"),
    ("=VLOOKUP({t},A:B,2)", "=VLOOKUP({t},A1:B5,2)"),
    ("=VLOOKUP({t},A1:B200,2,TRUE)", "=VLOOKUP({t},A1:B5,2,TRUE)"),
    ("=LOOKUP({t},A:A,B:B)", "=LOOKUP({t},A1:A5,B1:B5)"),
    ("=LOOKUP({t},A:B)", "=LOOKUP({t},A1:B5)"),
    ("=MATCH({t},A:A)", "=MATCH({t},A1:A5)"),
    ("=MATCH({t},A1:A200,1)", "=MATCH({t},A1:A5,1)"),
    ("=VLOOKUP({t},Z:AA,2,TRUE)", "#N/A"),
    ("=MATCH({t},Z:Z,1)", "#N/A"),
];

const TEXT_PAIRS: [(&str, &str); 2] = [
    ("=VLOOKUP({t},D:E,2,TRUE)", "=VLOOKUP({t},D1:E5,2,TRUE)"),
    ("=MATCH({t},D:D,1)", "=MATCH({t},D1:D5,1)"),
];

const DESCENDING_PAIRS: [(&str, &str); 2] = [
    ("=MATCH({t},G:G,-1)", "=MATCH({t},G1:G5,-1)"),
    ("=MATCH({t},G1:G200,-1)", "=MATCH({t},G1:G5,-1)"),
];

fn mismatches(
    model: &mut crate::Model,
    pairs: &[(&str, &str)],
    targets: &[&str],
    first_row: usize,
) -> Vec<String> {
    let mut cells = Vec::new();
    let mut row = first_row;
    for (whole, filled) in pairs {
        for target in targets {
            let whole = whole.replace("{t}", target);
            let filled = filled.replace("{t}", target);
            model._set(&format!("M{row}"), &whole);
            if filled.starts_with('=') {
                model._set(&format!("N{row}"), &filled);
            }
            cells.push((row, whole, filled));
            row += 1;
        }
    }
    model.evaluate();
    cells
        .into_iter()
        .filter_map(|(row, whole, filled)| {
            let got = model._get_text(&format!("M{row}"));
            let expected = if filled.starts_with('=') {
                model._get_text(&format!("N{row}"))
            } else {
                filled.clone()
            };
            (got != expected).then(|| format!("{whole} gave {got}, {filled} gives {expected}"))
        })
        .collect()
}

#[test]
fn a_sorted_lookup_ignores_the_empty_cells_after_the_last_value() {
    let mut model = new_empty_model();
    let fruit = ["apple", "banana", "cherry", "date", "fig"];
    for row in 1..=5 {
        model._set(&format!("A{row}"), &format!("{row}"));
        model._set(&format!("B{row}"), &format!("{}", row * 10));
        model._set(&format!("D{row}"), fruit[row - 1]);
        model._set(&format!("E{row}"), &format!("{row}"));
        model._set(&format!("G{row}"), &format!("{}", 60 - row * 10));
    }
    let mut wrong = mismatches(&mut model, &PAIRS, &TARGETS, 1);
    wrong.extend(mismatches(&mut model, &TEXT_PAIRS, &TEXT_TARGETS, 100));
    wrong.extend(mismatches(
        &mut model,
        &DESCENDING_PAIRS,
        &DESCENDING_TARGETS,
        150,
    ));
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_sorted_hlookup_ignores_the_empty_cells_after_the_last_value() {
    let mut model = new_empty_model();
    for column in 1..=5 {
        let name = (b'A' + column as u8 - 1) as char;
        model._set(&format!("{name}1"), &format!("{column}"));
        model._set(&format!("{name}2"), &format!("{}", column * 10));
    }
    let mut row = 10;
    let mut wrong = Vec::new();
    for target in TARGETS {
        model._set(
            &format!("A{row}"),
            &format!("=HLOOKUP({target},1:2,2,TRUE)"),
        );
        model._set(
            &format!("B{row}"),
            &format!("=HLOOKUP({target},A1:E2,2,TRUE)"),
        );
        row += 1;
    }
    model.evaluate();
    for row in 10..row {
        let (got, expected) = (
            model._get_text(&format!("A{row}")),
            model._get_text(&format!("B{row}")),
        );
        if got != expected {
            wrong.push(format!(
                "row {row}: whole row gave {got}, A1:E2 gives {expected}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn an_approximate_match_over_a_whole_column_agrees_with_excel() {
    let mut model = new_empty_model();
    for (row, (up, down)) in [(10, 30), (20, 20), (30, 10)].iter().enumerate() {
        model._set(&format!("A{}", row + 1), &format!("{up}"));
        model._set(&format!("B{}", row + 1), &format!("{down}"));
    }
    model._set("C1", "=MATCH(99,A:A,1)");
    model._set("C2", "=MATCH(-99,B:B,-1)");
    model._set("C3", "=MATCH(25,A:A)");
    model._set("H1000", "1");
    model._set("C2000", "2");
    model.evaluate();
    assert_eq!(model._get_text("C1"), "3");
    assert_eq!(model._get_text("C2"), "3");
    assert_eq!(model._get_text("C3"), "2");
}
