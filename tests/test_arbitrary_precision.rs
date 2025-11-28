#![cfg(feature = "arbitrary_precision")]
#![allow(clippy::unreadable_literal)]

use serde_yaml_ng::{Number, Value};
use std::str::FromStr;

#[test]
fn test_arbitrary_precision_from_str() {
    let huge_number = "999999999999999999999999999999999999999999";

    let number = Number::from_str(huge_number).unwrap();
    assert!(number.is_arbitrary_precision());
    assert_eq!(number.as_arbitrary_precision(), Some(huge_number));

    let value = Value::Number(number);
    let serialized = serde_yaml_ng::to_string(&value).unwrap();
    assert!(serialized.contains(huge_number));
}

#[test]
fn test_arbitrary_precision_negative_from_str() {
    let huge_negative = "-999999999999999999999999999999999999999999";
    let number = Number::from_str(huge_negative).unwrap();
    assert!(number.is_arbitrary_precision());
    assert_eq!(number.as_arbitrary_precision(), Some(huge_negative));
}

#[test]
fn test_arbitrary_precision_hex_from_str() {
    let hex = "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";
    let number = Number::from_str(hex).unwrap();
    assert!(number.is_arbitrary_precision());
}

#[test]
#[cfg(feature = "128bit-support")]
fn test_u128_not_arbitrary() {
    let u128_max_str = "340282366920938463463374607431768211455";
    let number = Number::from_str(u128_max_str).unwrap();
    assert!(!number.is_arbitrary_precision());
    assert_eq!(number.as_u128(), Some(u128::MAX));
}
