use csv::{Reader, StringRecord};

fn parse(input: &str) -> (StringRecord, Vec<StringRecord>) {
    let mut reader = Reader::from_reader(input.as_bytes());
    (
        reader.headers().unwrap().clone(),
        reader.records().map(Result::unwrap).collect(),
    )
}

#[test]
fn fabricated_orders_have_18_rows_and_no_customer_values_reach_the_oracle() {
    let (input_header, input) = parse(include_str!("../../../fixtures/orders/input.csv"));
    let (output_header, output) = parse(include_str!("../../../fixtures/orders/expected.csv"));
    assert_eq!(
        input_header,
        StringRecord::from(vec!["order_id", "created", "amount", "customer"])
    );
    assert_eq!(
        output_header,
        StringRecord::from(vec!["Order", "Date", "Amount"])
    );
    assert_eq!(input.len(), 18);
    assert_eq!(output.len(), 18);
    assert!(input.iter().any(|row| row[3].contains(',')));
    assert!(input.iter().any(|row| row[3].starts_with('=')));
    assert_eq!(&input[0][2], "$1,240.00");
    assert_eq!(&output[0][2], "1240.00");
    for (source, expected) in input.iter().zip(&output) {
        assert_eq!(&source[0], &expected[0]);
        assert_eq!(expected.len(), 3);
        assert_eq!(expected[1].len(), 10);
        assert_eq!(&expected[1][4..5], "-");
        assert_eq!(&expected[1][7..8], "-");
        assert_eq!(expected[2].split('.').nth(1).unwrap().len(), 2);
        assert!(expected[2]
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.'));
        for customer in &input {
            assert!(!expected.iter().any(|field| field.contains(&customer[3])));
        }
        assert!(!expected
            .iter()
            .any(|field| field.starts_with(['=', '+', '-', '@', '\''])));
    }
}

#[test]
fn negative_fixtures_preserve_their_intended_failure_triggers() {
    let (_, bad_date) = parse(include_str!(
        "../../../fixtures/orders/negative/bad-date.csv"
    ));
    assert_eq!(&bad_date[0][1], "13/45/26");
    let (_, missing_amount) = parse(include_str!(
        "../../../fixtures/orders/negative/missing-amount.csv"
    ));
    assert!(missing_amount[0][2].is_empty());
    let (missing_header, _) = parse(include_str!(
        "../../../fixtures/orders/negative/missing-column.csv"
    ));
    assert!(!missing_header.iter().any(|column| column == "created"));
    let (header, empty) = parse(include_str!("../../../fixtures/orders/negative/empty.csv"));
    assert_eq!(header.len(), 4);
    assert!(empty.is_empty());
}
