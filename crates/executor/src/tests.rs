use super::*;
use macroscope_domain::OutputColumn;

fn mapping(transform: Transform) -> CsvMapping {
    CsvMapping {
        output_columns: vec![OutputColumn {
            name: "out".into(),
            source: "in".into(),
            transform,
        }],
        on_error: macroscope_domain::OnError::StopAndExplain,
        skip_header_rows: 0,
        formula_escape: true,
    }
}

#[test]
fn orders_exact_bytes_and_all_negative_fixtures() {
    let contract: TaskContract =
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
    validate_mapping(&contract.mapping, &contract).unwrap();
    let result = apply_mapping(
        &contract.mapping,
        include_bytes!("../../../fixtures/orders/input.csv"),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(
        result.bytes,
        include_bytes!("../../../fixtures/orders/expected.csv")
    );
    assert_eq!((result.rows_in, result.rows_out), (18, 18));
    assert_eq!(
        result.sha256,
        format!("{:x}", Sha256::digest(&result.bytes))
    );
    assert!(matches!(
        apply_mapping(
            &contract.mapping,
            include_bytes!("../../../fixtures/orders/negative/empty.csv"),
            &Limits::default()
        ),
        Err(MappingError::EmptyInput)
    ));
    assert!(
        matches!(apply_mapping(&contract.mapping, include_bytes!("../../../fixtures/orders/negative/missing-column.csv"), &Limits::default()), Err(MappingError::MissingColumns(c)) if c == ["created"])
    );
    for (input, column) in [
        (
            include_bytes!("../../../fixtures/orders/negative/bad-date.csv").as_slice(),
            "created",
        ),
        (
            include_bytes!("../../../fixtures/orders/negative/missing-amount.csv").as_slice(),
            "amount",
        ),
    ] {
        assert!(
            matches!(apply_mapping(&contract.mapping,input,&Limits::default()), Err(MappingError::Row { row_number:1, column:c, .. }) if c==column)
        );
    }
}

#[test]
fn decimal_money_examples_and_properties() {
    for (input, expected) in [
        ("$1,240.00", "1240.00"),
        ("(5.10)", "-5.10"),
        ("1.005", "1.01"),
        ("-1.005", "-1.01"),
        ("9007199254740993.01", "9007199254740993.01"),
        ("999.999", "1000.00"),
        ("-0.001", "0.00"),
        ("($1,000.50)", "-1000.50"),
    ] {
        assert_eq!(money(input, "USD", 2).unwrap(), expected);
    }
    for bad in [
        "", " ", "1,2,3", "12,34", "1,0000", "$£1", "$$1", "€1", "NaN", "1e3", "--1", "(-1)", "1.",
        ".", "(1", "1)",
    ] {
        assert!(money(bad, "USD", 2).is_err(), "{bad}");
    }
    assert_eq!(money("€1,000.50", "EUR", 2).unwrap(), "1000.50");
    assert_eq!(money("£-5.15", "GBP", 1).unwrap(), "-5.2");
    for whole in 0..1000 {
        for fraction in [0, 1, 4, 5, 9, 49, 50, 99, 499, 500, 999] {
            let text = format!("{whole}.{fraction:03}");
            let cents = whole * 100 + (fraction + 5) / 10;
            let rounded = money(&text, "USD", 2).unwrap();
            assert_eq!(rounded, format!("{}.{:02}", cents / 100, cents % 100));
            assert_eq!(money(&rounded, "USD", 2).unwrap(), rounded);
            if cents > 0 {
                assert_eq!(
                    money(&format!("({text})"), "USD", 2).unwrap(),
                    format!("-{rounded}")
                );
            }
        }
    }
}

#[test]
fn fixed_dates_calendar_and_century_properties() {
    use DateFormat::*;
    for year in 2000..=2099 {
        for month in 1..=12 {
            let input = format!("{month:02}/01/{:02}", year - 2000);
            let iso = date(&input, MmDdYy, YyyyMmDd).unwrap();
            assert_eq!(iso, format!("{year:04}-{month:02}-01"));
            assert_eq!(date(&iso, Iso8601, MmDdYy).unwrap(), input);
            for format in [MmDdYyyy, DdMmYyyy, YyyyMmDd, Iso8601] {
                let encoded = date(&iso, YyyyMmDd, format).unwrap();
                assert_eq!(date(&encoded, format, YyyyMmDd).unwrap(), iso);
            }
        }
    }
    for invalid in [
        "13/45/26",
        "02/29/26",
        "04/31/26",
        "00/01/26",
        "01/00/26",
        "1/01/26",
        "01/01/2026",
        " 01/01/26",
    ] {
        assert!(date(invalid, MmDdYy, YyyyMmDd).is_err());
    }
    assert!(date("02/29/00", MmDdYy, Iso8601).is_ok());
    assert_eq!(date("03/04/26", DdMmYy, Iso8601).unwrap(), "2026-04-03");
    assert_eq!(date("03/04/26", MmDdYy, Iso8601).unwrap(), "2026-03-04");
    assert!(date("25/12/26", MmDdYy, Iso8601).is_err());
    assert_eq!(date("25/12/26", DdMmYy, YyyyMmDd).unwrap(), "2026-12-25");
    assert!(date("1900-02-29", Iso8601, MmDdYyyy).is_err());
    assert!(date("2100-01-01", Iso8601, MmDdYy).is_err());
}

#[test]
fn formula_and_quoting_roundtrip() {
    for value in [
        "=SUM(A1)",
        "+cmd",
        "-cmd",
        "@x",
        "\tfoo",
        "\rfoo",
        "<script>x</script>",
        "comma,quote\"\nnewline",
        "é日",
        "-12.50",
        "+12",
        "42",
    ] {
        let mut input = csv::Writer::from_writer(Vec::new());
        input.write_record(["in"]).unwrap();
        input.write_record([value]).unwrap();
        let output = apply_mapping(
            &mapping(Transform::Copy),
            &input.into_inner().unwrap(),
            &Limits::default(),
        )
        .unwrap();
        let mut csv = csv::Reader::from_reader(output.bytes.as_slice());
        let record = csv.records().next().unwrap().unwrap();
        let expected =
            if value.starts_with(['=', '+', '-', '@', '\t', '\r']) && !decimal_number(value) {
                format!("'{value}")
            } else {
                value.into()
            };
        assert_eq!(&record[0], expected);
        let mut disabled = mapping(Transform::Copy);
        disabled.formula_escape = false;
        let input = format!("in\n{}\n", value.replace('\n', ""));
        if !value.contains([',', '"', '\r']) {
            assert!(apply_mapping(&disabled, input.as_bytes(), &Limits::default()).is_ok());
        }
    }
    assert_eq!(
        apply_mapping(
            &mapping(Transform::Copy),
            b"in\r\n\"a,b\"\r\n",
            &Limits::default()
        )
        .unwrap()
        .bytes,
        b"out\n\"a,b\"\n"
    );
}

#[test]
fn stop_on_first_bad_row_and_scrub_bounded_preview() {
    let mapping = mapping(Transform::Money {
        currency: "USD".into(),
        decimals: 2,
    });
    assert!(matches!(
        apply_mapping(&mapping, b"in\n1\ninvalid\n2\n", &Limits::default()),
        Err(MappingError::Row { row_number: 2, .. })
    ));
    let value = "é\n\r\t\u{1b}".repeat(30);
    let preview = preview(&value);
    assert!(preview.chars().count() <= 40);
    assert!(!preview.chars().any(char::is_control));
    assert!(preview.contains("\\n"));
}

#[test]
fn budgets_before_and_during_processing() {
    let m = mapping(Transform::Copy);
    for (limits, kind) in [
        (
            Limits {
                max_input_bytes: 4,
                ..Limits::default()
            },
            "input bytes",
        ),
        (
            Limits {
                max_rows: 1,
                ..Limits::default()
            },
            "rows",
        ),
        (
            Limits {
                max_output_bytes: 5,
                ..Limits::default()
            },
            "output bytes",
        ),
    ] {
        assert!(
            matches!(apply_mapping(&m,b"in\na\nb\n",&limits),Err(MappingError::Limit{kind:k,..}) if k==kind)
        );
    }
    let bytes = apply_mapping(&m, b"in\na\n", &Limits::default())
        .unwrap()
        .bytes;
    assert!(apply_mapping(
        &m,
        b"in\na\n",
        &Limits {
            max_input_bytes: 5,
            max_rows: 1,
            max_output_bytes: bytes.len() as u64
        }
    )
    .is_ok());
    assert!(matches!(
        apply_mapping(
            &m,
            b"in\n",
            &Limits {
                max_rows: 0,
                ..Limits::default()
            }
        ),
        Err(MappingError::EmptyInput)
    ));
    let huge = mapping(Transform::Constant {
        value: "x".repeat(30_000),
    });
    assert!(matches!(
        apply_mapping(
            &huge,
            b"in\na\n",
            &Limits {
                max_output_bytes: 100,
                ..Limits::default()
            }
        ),
        Err(MappingError::Limit {
            kind: "output bytes",
            ..
        })
    ));
}

#[test]
fn pure_transforms_preambles_and_candidate_validation() {
    for (transform, expected) in [
        (Transform::Trim, "aBc"),
        (Transform::Upper, " ABC "),
        (Transform::Lower, " abc "),
        (
            Transform::Constant {
                value: "fixed".into(),
            },
            "fixed",
        ),
    ] {
        let output =
            apply_mapping(&mapping(transform), b"in\n aBc \n", &Limits::default()).unwrap();
        assert_eq!(output.bytes, format!("out\n{expected}\n").as_bytes());
    }
    let mut m = mapping(Transform::Copy);
    m.skip_header_rows = 1;
    assert_eq!(
        apply_mapping(&m, b"preamble\nin\nx\n", &Limits::default())
            .unwrap()
            .bytes,
        b"out\nx\n"
    );
    let mut contract: TaskContract =
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
    let mut candidate = contract.mapping.clone();
    candidate.output_columns[0].source = "private".into();
    candidate.output_columns[1].name = String::new();
    candidate.output_columns[2].name = candidate.output_columns[0].name.clone();
    candidate.formula_escape = false;
    assert!(validate_mapping(&candidate, &contract).unwrap_err().len() >= 5);
    contract.mapping.output_columns[0].transform = Transform::Constant {
        value: "fixed".into(),
    };
    contract.mapping.output_columns[0].source = "ignored".into();
    validate_mapping(&contract.mapping, &contract).unwrap();
    assert!(
        matches!(require_columns(b"order_id,created,amount\nx,x,x\n",0,&contract.input_schema),Err(MappingError::MissingColumns(c)) if c==["customer"])
    );
    assert!(matches!(
        apply_mapping(
            &mapping(Transform::Copy),
            b"in,in\na,b\n",
            &Limits::default()
        ),
        Err(MappingError::Csv(_))
    ));
    assert!(matches!(
        apply_mapping(&mapping(Transform::Copy), b"in\na,b\n", &Limits::default()),
        Err(MappingError::Row { row_number: 1, .. })
    ));
}
