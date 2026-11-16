use crate::test::util::new_empty_model;

#[test]
fn lookups_over_the_same_range_share_one_read() {
    let mut model = new_empty_model();
    for row in 1..=5 {
        model._set(&format!("A{row}"), &format!("k{row}"));
        model._set(&format!("B{row}"), &format!("{}", row * 10));
    }
    model._set("C1", "=VLOOKUP(\"k5\",A:B,2,FALSE)");
    model._set("C2", "=VLOOKUP(\"k1\",A:B,2,FALSE)");
    model._set("C3", "=INDEX(B:B,MATCH(\"k3\",A:A,0))");
    model._set("C4", "=XLOOKUP(\"k2\",A:A,B:B)");
    model._set("C5", "=XMATCH(\"k4\",A:A)");
    model._set("C6", "=VLOOKUP(\"k4\",A1:B3,2,FALSE)");
    model.evaluate();
    assert_eq!(model._get_text("C1"), "50");
    assert_eq!(model._get_text("C2"), "10");
    assert_eq!(model._get_text("C3"), "30");
    assert_eq!(model._get_text("C4"), "20");
    assert_eq!(model._get_text("C5"), "4");
    assert_eq!(model._get_text("C6"), "#N/A");
    assert_eq!(model.evaluation.lookup_vectors_read, 2);
}

#[test]
fn a_spill_into_a_shared_range_reaches_every_lookup() {
    let mut model = new_empty_model();
    model._set("A1", "=SEQUENCE(1,1,MATCH(\"x\",B2:B10,0))");
    model._set("A2", "=SEQUENCE(1,1,MATCH(\"x\",B2:B10,0))");
    model._set("A3", "=MATCH(\"x\",B2:B10,0)");
    model._set("B1", "=IF(SEQUENCE(5)=5,\"x\",\"\")");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "4");
    assert_eq!(model._get_text("A2"), "4");
    assert_eq!(model._get_text("A3"), "4");
}

const COLUMN: [&str; 20] = [
    "Straße",
    "STRASSE",
    "ab",
    "AB",
    "a*b",
    "2",
    "=2+2^-51",
    "=123456789012345.6",
    "123456789012346",
    "-5",
    "0.5",
    "",
    "TRUE",
    "=\"5\"",
    "ab",
    "2",
    "=\"\"",
    "ß",
    "ss",
    "x",
];

const TARGETS: [&str; 25] = [
    "\"Straße\"",
    "\"STRASSE\"",
    "\"strasse\"",
    "\"ab\"",
    "\"Ab\"",
    "\"a*b\"",
    "\"a?\"",
    "2",
    "2+2^-51",
    "2-2^-52",
    "123456789012345.6",
    "123456789012346",
    "-5",
    "0.5",
    "0",
    "\"\"",
    "TRUE",
    "FALSE",
    "\"5\"",
    "5",
    "\"ß\"",
    "\"SS\"",
    "\"x\"",
    "\"nope\"",
    "A12",
];

const LOOKUPS: [&str; 6] = [
    "=VLOOKUP({t},{k}1:{v}20,2,FALSE)",
    "=MATCH({t},{k}1:{k}20,0)",
    "=XLOOKUP({t},{k}1:{k}20,{v}1:{v}20,\"none\")",
    "=XLOOKUP({t},{k}1:{k}20,{v}1:{v}20,\"none\",0,-1)",
    "=XMATCH({t},{k}1:{k}20)",
    "=XMATCH({t},{k}1:{k}20,0,-1)",
];

fn column_name(column: usize) -> String {
    let mut name = String::new();
    let mut rest = column;
    while rest > 0 {
        name.insert(0, (b'A' + ((rest - 1) % 26) as u8) as char);
        rest = (rest - 1) / 26;
    }
    name
}

fn lookups_into(model: &mut crate::Model, keys_at: impl Fn(usize) -> usize) -> Vec<String> {
    let mut cells = Vec::new();
    for (row, target) in TARGETS.iter().enumerate() {
        for (column, lookup) in LOOKUPS.iter().enumerate() {
            let keys = keys_at(row * LOOKUPS.len() + column);
            let (k, v) = (column_name(keys), column_name(keys + 1));
            for (at, value) in COLUMN.iter().enumerate() {
                if !value.is_empty() {
                    model._set(&format!("{k}{}", at + 1), value);
                }
                model._set(&format!("{v}{}", at + 1), &format!("{}", at + 1));
            }
            let formula = lookup
                .replace("{t}", target)
                .replace("{k}", &k)
                .replace("{v}", &v);
            let cell = format!("{}{}", column_name(5 + column), row + 1);
            model._set(&cell, &formula);
            cells.push(cell);
        }
    }
    model.evaluate();
    cells
        .iter()
        .map(|cell| format!("{cell} {}", model._get_text(cell)))
        .collect()
}

#[test]
fn an_indexed_lookup_finds_what_a_search_of_every_value_finds() {
    let mut shared = new_empty_model();
    let found = lookups_into(&mut shared, |_| 1);
    let mut searched = new_empty_model();
    let expected = lookups_into(&mut searched, |at| 11 + 2 * at);
    assert!(
        shared.evaluation.lookup_vectors_read * 10 < searched.evaluation.lookup_vectors_read,
        "the lookups over one range did not share it"
    );
    assert_eq!(found, expected);
}
