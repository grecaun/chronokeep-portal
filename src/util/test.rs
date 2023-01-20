use super::{Time, pretty_time, pretty_time_full};

#[test]
fn test_time_since() {
    let first = Time{
        seconds: 1000,
        milliseconds: 800,
    };
    let second = Time{
        seconds: 900,
        milliseconds: 800,
    };
    let third = Time{
        seconds: 1350,
        milliseconds: 850,
    };
    let fourth = Time{
        seconds: 1500,
        milliseconds: 223
    };
    let diff = first.time_since(&second);
    assert!(diff.is_ok());
    let diff = diff.unwrap();
    assert_eq!(100, diff.seconds);
    assert_eq!(0, diff.milliseconds);
    let diff = first.time_since(&third);
    assert!(diff.is_err());
    let diff = first.time_since(&fourth);
    assert!(diff.is_err());
    let diff = fourth.time_since(&third);
    assert!(diff.is_ok());
    let diff = diff.unwrap();
    assert_eq!(149, diff.seconds);
    assert_eq!(373, diff.milliseconds);
    let diff = third.time_since(&first);
    assert!(diff.is_ok());
    let diff = diff.unwrap();
    assert_eq!(350, diff.seconds);
    assert_eq!(50, diff.milliseconds);
}

#[test]
fn test_pretty_time_full() {
    // test invalid milliseconds
    let t = Time{
        seconds: 3*60,
        milliseconds: 1010,
    };
    let pretty = pretty_time_full(&t);
    assert!(pretty.is_err());
    // 3 minutes
    let t = Time{
        seconds: 3 * 60,
        milliseconds: 0,
    };
    let pretty = pretty_time_full(&t);
    assert!(pretty.is_ok());
    let pretty = pretty.unwrap();
    assert_eq!(String::from("0:03:00.000"), pretty);
    // 3 minutes 4 seconds
    let t = Time{
        seconds: 3 * 60 + 4,
        milliseconds: 0,
    };
    let pretty = pretty_time_full(&t);
    assert!(pretty.is_ok());
    let pretty = pretty.unwrap();
    assert_eq!(String::from("0:03:04.000"), pretty);
    // 24 minutes 34 seconds 104 milliseconds
    let t = Time{
        seconds: 26 * 60 + 34,
        milliseconds: 104,
    };
    let pretty = pretty_time_full(&t);
    assert!(pretty.is_ok());
    let pretty = pretty.unwrap();
    assert_eq!(String::from("0:26:34.104"), pretty);
    // 1 hour 24 minutes 34 seconds 104 milliseconds
    let t = Time{
        seconds: 1 * 3600 + (26 * 60) + 34,
        milliseconds: 104,
    };
    let pretty = pretty_time_full(&t);
    assert!(pretty.is_ok());
    let pretty = pretty.unwrap();
    assert_eq!(String::from("1:26:34.104"), pretty);
    // 12 hours 24 minutes 34 seconds 50 milliseconds
    let t = Time{
        seconds: 12 * 3600 + (26 * 60) + 34,
        milliseconds: 50,
    };
    let pretty = pretty_time_full(&t);
    assert!(pretty.is_ok());
    let pretty = pretty.unwrap();
    assert_eq!(String::from("12:26:34.050"), pretty);
}

#[test]
fn test_pretty_time() {
    let t: u64 = 3 * 60;
    let pretty = pretty_time(&t);
    assert_eq!(String::from("3:00"), pretty);
    let t: u64 = 3 * 60 + 4;
    let pretty = pretty_time(&t);
    assert_eq!(String::from("3:04"), pretty);
    let t: u64 = 26 * 60 + 34;
    let pretty = pretty_time(&t);
    assert_eq!(String::from("26:34"), pretty);
    let t: u64 = 1 * 3600 + (26 * 60) + 34;
    let pretty = pretty_time(&t);
    assert_eq!(String::from("1:26:34"), pretty);
    let t: u64 = 1 * 3600 + (6 * 60) + 4;
    let pretty = pretty_time(&t);
    assert_eq!(String::from("1:06:04"), pretty);
}