//! Fabricated screen text only. Each entry is a separately labelled screen line/block.
use super::*;
use legacy::LegacyDetector;

const HARMLESS: &[&str] = &[
    "Sold 30 Oct 7:03 pm",
    "5 items in the queue",
    "2 new Messages",
    "Qty 1 Sale price $49.00",
    "Level 3 Street Fighter",
    "1am2am3am4am5am6am7am8am9am10am11am12pm",
    "1am 2am 3am 4am 5am 6am 7am 8am 9am 10am 11am 12pm 1pm 2pm",
    "SW35SW39SW35SW37SW33SW31W28SW39SW43",
    "SW 35 SW 39 SW 35 SW 37 SW 33 SW 31 W 28 SW 39",
    "0 0 0 0 0 0.2 0 0 0 0 0 0 0 0 0 0 1.6 0 0 0",
    "Strong 41km/h W Gusts 69km/h Harbour Bridge Strong 46km/h SW",
    "Auckland Aero Automatic Weather Station (93110)",
    "Issued: 4:26pm Fri 30 Oct   Data provided by 90544",
    "Auckland 04:45 0.3m 11:12 3.4m 17:13 0.5m 23:38 3.2m",
    "Onehunga 01:53 4.0m 08:07 0.7m 14:21 4.0m 20:36 0.8m",
    "9:55 AM - 4:35 PM tomorrow.",
    "9am 10am 11am 12pm 1pm 2pm 3pm 4pm 5pm 6pm",
    "9 10 11 12 1 2 3 4 5 6 7 8",
    "12.8° 14.2° 15° 15° 17° 17° 18° 18° 17° 17°",
    "0 0 0 0 0 0 0 0 0 0 0 0 0",
    "28 39 34 34",
    "11 10 10 10 10 9 10",
    "0.0 0.0 0.0 0.0 0.2 0.4 1.2",
    "Fri 30 Oct Sat 31 Oct Sun 1 Nov Mon 2 Nov",
    "1013 1012 1011 1010 1009 1008",
    "15 17 18 20 21 22 20 18",
    "7 3 4 5 7 7 8 9 12 13 15",
    "2026 10 30 14 45 22",
    "0 0 0 0 0 0 0 0 0 0 0 0",
    "28 39 34 34 35 37 37 41 43 43 41 39",
    "14 14 13 13 12 12 11 10 10 10 10 9",
    "trademe.co.nz/a/marketplace/gaming?condition=used&buy=now&sort_order=ExpiryDesc",
    "https://www.trademe.co.nz/a/marketplace/gaming/listing/5583960712",
    "Closes: Thu 5th Nov, 11:45pm",
    "Buy now $400.00",
    "Or four interest-free payments of $100.00 (plus shipping)",
    "$4.99 Service Fee may apply",
    "Nintendo Switch Monster Hunter Rise Limited Edition",
    "Home",
    "Accounts",
    "Payments",
    "Transfers",
    "Statements",
    "Settings",
    "Welcome to Example Bank",
    "Everyday banking",
    "Mobile banking",
    "Routing guide",
    "Password and security",
    "Savings account",
    "Recent transactions",
    "Available balance",
    "Current balance: $1,234.56",
    "Amount: NZD 123.45",
    "$1234567890.12",
    "Total 1234567.89",
    "Interest rate: 4.50%",
    "Payment of $25.00",
    "Fee: $0.00",
    "29 October 2026",
    "2026-10-29",
    "28/10/2026",
    "10/29/2026",
    "29.10.2026",
    "10:45",
    "10:45:23",
    "9:30 AM",
    "Updated 2026-10-29 10:45",
    "2026/10/29",
    "Order 12345",
    "Invoice 123456",
    "Page 12 of 34",
    "Items: 123",
    "Version 1.2.345",
    "Report for 2026-10-28",
    "Report 123-45-678",
    "Card 4111 1111 1111 1112",
    "4111 1111",
    "1111 1111",
    "7 4111 1111 1111 1111",
    "GB report 82",
    "https://bank.example.test",
    "https://bank.example.test/accounts/overview",
    "bank.example.test/accounts/current/transactions?month=2026-10",
    "https://bank.example.test/reports/MonthlyFinancialStatement2026",
    "https://bank.example.test/a/very/long/ordinary/browser/path/to/reports",
    "https://example.test/search?q=ordinary+search+terms&page=2",
    "https://example.test/help/how-to-change-your-password",
    "https://example.test/docs/api-key-settings",
    "https://example.test/?sort=date&limit=100",
    "https://example.test/2026/10/29/1234567890",
    "https://example.test/?account=1234567890",
    "https://example.test/?redirect=/reports/MonthlyFinancialStatement2026",
    "example.com",
    "bank.example.co.uk/reports/1234567890",
    "bank.example.bank/reports/1234567890",
    "www.example.org/help",
    "localhost:3000/dashboard",
    "/Users/demo/Documents/MonthlyFinancialStatement2026.csv",
    "/reports/quarterly/AnnualFinancialPerformance2026",
    "~/Downloads/StatementExportForOctober2026.csv",
    "AnnualFinancialPerformance2026",
    "MonthlyFinancialStatement2026",
    "StatementExportForOctober2026",
    "aLongOrdinaryWordWithNoDigitsInIt",
    "Accessibility",
    "Internationalisation",
    "Navigation Preferences",
    "Password reset",
    "Password settings",
    "Password requirements",
    "Password manager",
    "Change password",
    "Reset your password",
    "Account number",
    "Card number",
    "Date of birth",
    "Tax number",
    "IBAN",
    "SWIFT",
    "Inbox (12)",
    "Sent mail",
    "Drafts",
    "Subject: Weekly project report",
    "Reply to all",
    "Archive message",
    "Good morning team",
    "Please review the attached report",
    "Meeting at 14:00 tomorrow",
    "Sent 2 hours ago",
    "A1: Revenue",
    "B2: 123.45",
    "SUM(A1:A10)",
    "Revenue2026",
    "Quarter 4",
    "2026 Q3 totals",
    "cargo test --offline",
    "Finished test profile in 12.34s",
    "src/main.rs:123:45",
    "HTTP 200 OK",
    "GET /api/reports?page=2",
    "npm run build",
    "git status --short",
    "Build 1234567",
    "sk-short",
    "AKIA_short",
    "This page has 24 results",
    "Report Export History",
    "No transactions to display",
    "Menu Help View Window",
    "https://example.test/?monkey=banana&postcode=6011",
    "https://example.test/?keyboard=standard",
    "https://example.test/?tokenizer=words&codepage=utf8",
    "https://example.test/?authorship=team",
];

const MUST_MASK: &[&str] = &[
    "100 Clarence Street",
    "Ship to 42 Queen St",
    "12B Great North Road",
    "3/45 Ponsonby Terrace",
    "221 Baker Street, London",
    "alex@example.test",
    "josé@example.test",
    "foo/bar=baz@example.test",
    "Contact user+tag@example.co.nz",
    "From: sam.lee@example.test",
    "To: billing@example.test",
    "CC: qa@example.test",
    "Reply: demo@example.org",
    "alex @ example.test",
    "alex@ example . test",
    "alex  @  example.test",
    "Card 4111 1111 1111 1111",
    "4111111111111111",
    "5555-5555-5555-4444",
    "3782 822463 10005",
    "6011 1111 1111 1117",
    "Card: 4111  1111  1111  1111",
    "4l11 1111 1111 1111",
    "4111 l111 1111 1111",
    "GB82 WEST 1234 5698 7654 32",
    "GB82WEST12345698765432",
    "DE89 3704 0044 0532 0130 00",
    "FR14 2004 1010 0505 0001 3M02 606",
    "NL91 ABNA 0417 1643 00",
    "GB82  WEST  1234  5698  7654  32",
    "123-45-6789",
    "SSN 234 56 7890",
    "987-65-4321",
    "123-45-678O",
    "12-3456-7890123-00",
    "01-0123-0123456-000",
    "12 3456 7890123 00",
    "12-3456-789O123-00",
    "12 - 3456 - 7890123 - 00",
    "123-456-785",
    "12-345-678",
    "IRD: 123456785",
    "123456785",
    "GB82 west 1234 5698 7654 32",
    "MU17 BOMM 0101 1010 3030 0200 000M UR",
    "Tax number: 01234567",
    "+64 21 123 4567",
    "(212) 555-0199",
    "021 123 4567",
    "09 555 1234",
    "+1 415 555 0100",
    "+44 20 7946 0123",
    "415-555-0199",
    "0211234567",
    "555-0123",
    "2125550199",
    "Call 021 123 4567",
    "Phone +64  21  123  4567",
    "+64 21 123 456O",
    "AKIAIOSFODNN7EXAMPLE",
    "ASIAIOSFODNN7EXAMPLE",
    "sk-test_abc123def456",
    "ghp_abc123def456ghi789",
    "github_pat_abc123def456ghi789",
    "aB12cd34ef56gh78ij90kl12mn34",
    "0123456789abcdef0123456789abcdef",
    "deadbeefdeadbeefdeadbeefdeadbeef",
    "QWxhZGRpbjpvcGVuIHNlc2FtZQ==",
    "QWxhZGRpbjpvcGVuIHNlc2FtZQ",
    "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkZW1vIn0.c3ludGhldGljc2lnbmF0dXJl",
    "Password: meadow flower",
    "Password hunter2",
    "PIN: 1234",
    "CVV: 123",
    "CVC: 456",
    "Account number: 1234567",
    "Account no: 1234567",
    "Card number: 123456789",
    "Routing: 021000021",
    "IBAN: damaged-value",
    "SWIFT: DEMONZ22",
    "Date of birth: 02/03/1990",
    "DOB: 1990-03-02",
    "Tax ID: 7654321",
    "SSN: OCR-damaged",
    "Passcode: abcd",
    "https://example.test/?token=abc",
    "https://example.test/?email=alex%40example.test",
    "https://example.test/?%74oken=abc",
    "https://example.test/?api_key=fabricated",
    "https://example.test/?secret=rose",
    "https://example.test/?password=fake",
    "https://example.test/?session=123",
    "https://example.test/?auth=demo",
    "https://example.test/?code=one-time",
    "https://example.test/?sig=signature",
    "https://example.test/path/aB12cd34ef56gh78ij90kl12mn34/view",
    "https://example.test/?email=alex@example.test",
    "export API_KEY=demo-value",
    "token=abc123",
    "https://example.test/?card=4111111111111111",
    "Phone number: 1234567",
];

fn screen(rows: &[&[&str]]) -> RecordedFrame {
    let mut f = super::super::tests::frame();
    f.width = 900;
    f.height = 120;
    f.rgba = [10, 20, 30, 255].repeat((f.width * f.height) as usize);
    f.ocr_boxes.clear();
    for (row, fragments) in rows.iter().enumerate() {
        let mut x = 12;
        for (column, text) in fragments.iter().enumerate() {
            let width = (text.chars().count() as u32 * 6).max(6);
            f.ocr_boxes.push(OcrBox {
                text: (*text).into(),
                rect: Rect {
                    x,
                    y: 12 + row as u32 * 26 + column as u32 % 2,
                    width,
                    height: 12,
                },
            });
            x += width + 6;
        }
    }
    f
}

fn old_detects(f: &RecordedFrame, d: &impl SensitiveDetector) -> bool {
    if f.ocr_boxes.iter().any(|b| d.is_sensitive(&b.text).unwrap()) {
        return true;
    }
    for line in ocr_lines(&f.ocr_boxes) {
        let mut runs = vec![line.as_slice()];
        runs.extend(line.windows(2));
        for run in runs {
            if run.is_empty() {
                continue;
            }
            let (joined, _) = run_text(&f.ocr_boxes, run, false).unwrap();
            let (compact, _) = run_text(&f.ocr_boxes, run, true).unwrap();
            if d.is_sensitive(&joined).unwrap() || d.is_sensitive(&compact).unwrap() {
                return true;
            }
        }
    }
    false
}

#[derive(Default)]
struct Score {
    tp: usize,
    fp: usize,
    tn: usize,
    misses: usize,
}
impl Score {
    fn add(&mut self, expected: bool, actual: bool) {
        match (expected, actual) {
            (true, true) => self.tp += 1,
            (false, true) => self.fp += 1,
            (false, false) => self.tn += 1,
            (true, false) => self.misses += 1,
        }
    }
    fn precision(&self) -> f64 {
        self.tp as f64 / (self.tp + self.fp).max(1) as f64
    }
    fn recall(&self) -> f64 {
        self.tp as f64 / (self.tp + self.misses).max(1) as f64
    }
    fn specificity(&self) -> f64 {
        self.tn as f64 / (self.tn + self.fp).max(1) as f64
    }
    fn print(&self, name: &str) {
        println!("{name}: precision={:.2}%, recall={:.2}%, harmless specificity={:.2}% (TP={}, FP={}, TN={}, FN={})",
            self.precision()*100., self.recall()*100., self.specificity()*100., self.tp, self.fp, self.tn, self.misses);
    }
}

#[test]
fn labelled_screen_corpus_old_vs_new() {
    let old = LegacyDetector::new().unwrap();
    let new = TextDetector::new().unwrap();
    let mut before = Score::default();
    let mut after = Score::default();
    let mut failures = Vec::new();
    let mut check = |rows: &[&[&str]], sensitive: bool| {
        let f = screen(rows);
        before.add(sensitive, old_detects(&f, &old));
        let kept = mask(
            f,
            &[],
            &new,
            &mut MaskHistory::default(),
            MaskLevel::Standard,
        )
        .unwrap();
        let actual = !kept.text().masks.is_empty();
        after.add(sensitive, actual);
        if sensitive != actual {
            failures.push(
                rows.iter()
                    .map(|r| r.join(" | "))
                    .collect::<Vec<_>>()
                    .join(" / "),
            );
        }
    };
    for &line in HARMLESS {
        check(&[&[line]], false);
    }
    for &line in MUST_MASK {
        check(&[&[line]], true);
    }
    // Different OCR partitioning, spaces and vertical offsets for the same fabricated screen.
    for fragments in [
        &["alex@", "example.test"][..],
        &["alex@exam", "ple.test"],
        &["alex", "@example.test"],
        &["4111", "1111", "1111", "1111"],
        &["4111 1111", "1111 1111"],
        &["4l11", "1111", "1111", "1111"],
        &["GB82", "WEST", "1234", "5698", "7654", "32"],
        &["12-3456-", "7890123-00"],
        &["+64", "21", "123", "4567"],
        &["aB12cd34ef56", "gh78ij90kl12mn34"],
        &["Password", "meadow"],
        &["Account", "number:", "1234567"],
        &["Date of birth", "02/03/1990"],
        &["Contact", "alex@example.test", "Reply"],
        &["https://example.test/?apiKey=", "fake-value"],
        &[
            "https://example.test/path/",
            "aB12cd34ef56",
            "gh78ij90kl12mn34",
        ],
    ] {
        check(&[fragments], true);
    }
    for (label, value) in [
        ("Password", "meadow"),
        ("PIN", "1234"),
        ("CVV", "123"),
        ("Account number", "1234567"),
        ("Date of birth", "1990-03-02"),
        ("Tax number", "1234567"),
        ("SWIFT", "DEMONZ22"),
        ("Card number", "4111 l111 1111 1111"),
        ("IBAN", "GB82 WEST 1234 5698 7654 32"),
    ] {
        check(&[&[label], &[value]], true);
    }
    for row in [
        &["Home", "Accounts", "Transfers"][..],
        &["Reports", "October", "2026"],
        &["https://bank.example.test", "/accounts/overview"],
        &["Menu", "View", "Help"],
        &["MonthlyFinancial", "Statement2026"],
        &["2026-10-29", "10:45"],
        &["https://example.test/reports/", "1234567890"],
        &["https://example.test/?account=", "1234567890"],
        &["bank.example.test/reports/", "1234567890"],
        &["bank.example.co.uk/reports/", "1234567890"],
    ] {
        check(&[row], false);
    }
    before.print("old");
    after.print("new");
    println!("fabricated corpus mismatches: {failures:?}");
    println!(
        "corpus: {} labelled lines/blocks",
        after.tp + after.fp + after.tn + after.misses
    );
    assert!(HARMLESS.len() + MUST_MASK.len() >= 150);
    assert_eq!(
        after.misses, 0,
        "must-mask misses; mismatches: {failures:?}"
    );
    assert!(after.precision() >= 0.95, "mismatches: {failures:?}");
    assert!(after.specificity() >= 0.95, "mismatches: {failures:?}");
}

fn covered(masks: &[MaskRegion], rect: Rect) -> bool {
    (rect.y..rect.y + rect.height).all(|y| {
        (rect.x..rect.x + rect.width).all(|x| {
            masks.iter().any(|m| {
                x >= m.rect.x
                    && x < m.rect.x + m.rect.width
                    && y >= m.rect.y
                    && y < m.rect.y + m.rect.height
            })
        })
    })
}

#[test]
fn spans_preserve_labels_and_neighbours_and_burn_every_sensitive_pixel() {
    let detector = TextDetector::new().unwrap();
    for (prefix, secret, suffix) in [
        (
            "https://example.test/?email=",
            "alex%40example.test",
            "&page=1",
        ),
        ("Contact ", "alex@example.test", " for support"),
        ("Password: ", "meadow", ""),
        ("Balance $12.34 Card ", "4111 1111 1111 1111", " Paid"),
        ("https://example.test/?token=", "fake-value", "&page=1"),
        (
            "https://example.test/?email=",
            "alex@example.test",
            "&page=1",
        ),
        ("Résumé café ", "josé@example.test", " terminé"),
    ] {
        let text = format!("{prefix}{secret}{suffix}");
        let f = screen(&[&[&text]]);
        let b = &f.ocr_boxes[0];
        let expected = sub_rect(b, prefix.len()..prefix.len() + secret.len());
        let masked = mask(
            f,
            &[],
            &detector,
            &mut MaskHistory::default(),
            MaskLevel::Standard,
        )
        .unwrap();
        let masks = &masked.text().masks;
        assert!(covered(masks, expected));
        assert_eq!(masks.len(), 1, "{text}");
        assert_eq!(masks[0].rect, padded_rect(expected, 900, 120), "{text}");
        // A partially masked raw box must be omitted in its entirety from metadata.
        assert!(masked.text().ocr_boxes.is_empty());
        let mut reader = png::Decoder::new(std::io::Cursor::new(masked.png()))
            .read_info()
            .unwrap();
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut decoded).unwrap();
        for y in 0..120 {
            for x in 0..900 {
                let offset = ((y * 900 + x) * 4) as usize;
                let r = masks[0].rect;
                let burn = x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
                assert_eq!(
                    &decoded[offset..offset + 4],
                    if burn {
                        &[0, 0, 0, 255]
                    } else {
                        &[10, 20, 30, 255]
                    }
                );
            }
        }
    }
    for rows in [
        vec![&["Account number", "1234567"][..]],
        vec![&["Account number"][..], &["1234567"]],
    ] {
        let f = screen(&rows);
        let label = f.ocr_boxes[0].rect;
        let value = f.ocr_boxes[1].rect;
        let kept = mask(
            f,
            &[],
            &detector,
            &mut MaskHistory::default(),
            MaskLevel::Standard,
        )
        .unwrap();
        assert!(covered(&kept.text().masks, value));
        assert!(!kept.text().masks.iter().any(|m| m.rect.intersects(label)));
        assert_eq!(kept.text().ocr_boxes[0].text, "Account number");
    }
}

#[test]
fn recent_masks_survive_split_merge_and_ocr_damage_and_expire() {
    let d = TextDetector::new().unwrap();
    let mut history = MaskHistory::default();
    let mut first = screen(&[&["Card", "4111 1111", "1111 1111"]]);
    first.mono_ms = 1000;
    let first = mask(first, &[], &d, &mut history, MaskLevel::Standard).unwrap();
    assert_eq!(first.text().masks.len(), 2);
    for (seq, ms, text, should_mask) in [
        (2, 2000, "Card 4111 1111 1111 1112", true),
        (3, 3000, "Card 4111 1111 1111 1112", true),
        (4, 4000, "Card 4111 1111 1111 1112", true),
        (5, 5000, "Card 4111 1111 1111 1112", false),
    ] {
        let mut next = screen(&[&[text]]);
        next.seq = seq;
        next.mono_ms = ms;
        next.uncertain = true;
        let kept = mask(next, &[], &d, &mut history, MaskLevel::Standard).unwrap();
        assert_eq!(!kept.text().masks.is_empty(), should_mask, "frame {seq}");
        assert!(kept.unverified());
        if should_mask {
            assert!(kept.text().ocr_boxes.is_empty());
            assert!(kept.text().ax_labels.is_empty());
            for m in &first.text().masks {
                assert!(covered(&kept.text().masks, m.rect));
            }
        }
    }
}

#[test]
fn history_is_bounded_to_nearby_similar_content_in_this_session_and_geometry() {
    let d = TextDetector::new().unwrap();
    for change in [
        "text",
        "position",
        "dimensions",
        "app",
        "time",
        "sequence",
        "session",
    ] {
        let mut history = MaskHistory::default();
        let mut first = screen(&[&["4111 1111 1111 1111"]]);
        first.mono_ms = 1000;
        mask(first, &[], &d, &mut history, MaskLevel::Standard).unwrap();
        let mut next = screen(&[&["4111 1111 1111 1112"]]);
        next.seq = 2;
        next.mono_ms = 2000;
        match change {
            "text" => next.ocr_boxes[0].text = "Menu Help View Window".into(),
            "position" => next.ocr_boxes[0].rect.y += 30,
            "dimensions" => {
                next.width += 1;
                next.rgba = [0, 0, 0, 255].repeat((next.width * next.height) as usize);
            }
            "app" => next.app_name = "Other App".into(),
            "time" => next.mono_ms = 4001,
            "sequence" => next.seq = 5,
            "session" => history = MaskHistory::default(),
            _ => unreachable!(),
        }
        let kept = mask(next, &[], &d, &mut history, MaskLevel::Standard).unwrap();
        assert!(kept.text().masks.is_empty(), "{change}");
    }
}

#[test]
fn jittered_geometry_and_confusion_do_not_uncover_old_sensitive_regions() {
    let d = TextDetector::new().unwrap();
    let mut history = MaskHistory::default();
    let mut first = screen(&[&["4111 1111 1111 1111"]]);
    first.mono_ms = 1000;
    let prior = mask(first, &[], &d, &mut history, MaskLevel::Standard)
        .unwrap()
        .text()
        .masks
        .clone();
    let mut next = screen(&[&["4l11 1111 1111 1112"]]);
    next.ocr_boxes[0].rect.x += 2;
    next.ocr_boxes[0].rect.y += 1;
    next.seq = 2;
    next.mono_ms = 2000;
    next.uncertain = true;
    let kept = mask(next, &[], &d, &mut history, MaskLevel::Standard).unwrap();
    assert!(kept.unverified());
    for m in prior {
        assert!(covered(&kept.text().masks, m.rect));
    }
}

#[test]
fn invalid_adapter_spans_and_errors_never_publish_a_frame_or_history() {
    struct Invalid;
    impl SensitiveDetector for Invalid {
        fn is_sensitive(&self, _: &str) -> Result<bool, DetectorError> {
            Ok(false)
        }
        #[allow(clippy::single_range_in_vec_init)]
        fn sensitive_ranges(&self, _: &str) -> Result<Vec<Range<usize>>, DetectorError> {
            Ok(vec![1..2])
        }
    }
    let mut history = MaskHistory::default();
    assert!(matches!(
        mask(
            screen(&[&["é"]]),
            &[],
            &Invalid,
            &mut history,
            MaskLevel::Standard
        ),
        Err(DropReason::DetectorError)
    ));
}

#[test]
fn multiple_labelled_fields_leave_each_label_visible() {
    let d = TextDetector::new().unwrap();
    for rows in [
        vec![&["PIN:", "1234", "CVV:", "456"][..]],
        vec![&["PIN", "CVV"][..], &["1234", "456"]],
    ] {
        let mut f = screen(&rows);
        if rows.len() == 2 {
            f.ocr_boxes[1].rect.x = 300;
            f.ocr_boxes[3].rect.x = 300;
        }
        let expected: Vec<_> = f
            .ocr_boxes
            .iter()
            .filter(|b| b.text.starts_with("PIN") || b.text.starts_with("CVV"))
            .map(|b| (b.text.clone(), b.rect))
            .collect();
        let kept = mask(f, &[], &d, &mut MaskHistory::default(), MaskLevel::Standard).unwrap();
        for (text, rect) in expected {
            assert!(
                !kept.text().masks.iter().any(|m| m.rect.intersects(rect)),
                "{text}"
            );
            assert!(kept.text().ocr_boxes.iter().any(|b| b.text == text));
        }
        assert!(!kept
            .text()
            .ocr_boxes
            .iter()
            .any(|b| b.text == "1234" || b.text == "456"));
    }
}

#[test]
fn postcodes_after_a_place_name_are_hidden_on_strict_only() {
    let strict = TextDetector::with_level(MaskLevel::Strict).unwrap();
    let standard = TextDetector::with_level(MaskLevel::Standard).unwrap();
    for line in ["Auckland 1011", "Hamilton East 3216", "NY 10001"] {
        assert!(strict.is_sensitive(line).unwrap(), "{line}");
        assert!(!standard.is_sensitive(line).unwrap(), "{line}");
    }
    assert!(!strict.is_sensitive("Season 2024").unwrap());
}
