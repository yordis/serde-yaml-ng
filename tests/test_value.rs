#![allow(
    clippy::derive_partial_eq_without_eq,
    clippy::eq_op,
    clippy::uninlined_format_args
)]

use indoc::indoc;
use serde::de::IntoDeserializer;
use serde::Deserialize;
use serde_derive::{Deserialize, Serialize};
use serde_yaml_ng::{Number, Value};

#[test]
fn test_nan() {
    let pos_nan = serde_yaml_ng::from_str::<Value>(".nan").unwrap();
    assert!(pos_nan.is_f64());
    assert_eq!(pos_nan, pos_nan);

    let neg_fake_nan = serde_yaml_ng::from_str::<Value>("-.nan").unwrap();
    assert!(neg_fake_nan.is_string());

    let significand_mask = 0xF_FFFF_FFFF_FFFF;
    let bits = (f64::NAN.copysign(1.0).to_bits() ^ significand_mask) | 1;
    let different_pos_nan = Value::Number(Number::from(f64::from_bits(bits)));
    assert_eq!(pos_nan, different_pos_nan);
}

#[test]
fn test_digits() {
    let num_string = serde_yaml_ng::from_str::<Value>("01").unwrap();
    assert!(num_string.is_string());
}

#[test]
fn test_into_deserializer() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Test {
        first: String,
        second: u32,
    }

    let value = serde_yaml_ng::from_str::<Value>("xyz").unwrap();
    let s = String::deserialize(value.into_deserializer()).unwrap();
    assert_eq!(s, "xyz");

    let value = serde_yaml_ng::from_str::<Value>("- first\n- second\n- third").unwrap();
    let arr = Vec::<String>::deserialize(value.into_deserializer()).unwrap();
    assert_eq!(arr, &["first", "second", "third"]);

    let value = serde_yaml_ng::from_str::<Value>("first: abc\nsecond: 99").unwrap();
    let test = Test::deserialize(value.into_deserializer()).unwrap();
    assert_eq!(
        test,
        Test {
            first: "abc".to_string(),
            second: 99
        }
    );
}

#[test]
fn test_merge() {
    // From https://yaml.org/type/merge.html.
    let yaml = indoc! {"
        ---
        - &CENTER { x: 1, y: 2 }
        - &LEFT { x: 0, y: 2 }
        - &BIG { r: 10 }
        - &SMALL { r: 1 }

        # All the following maps are equal:

        - # Explicit keys
          x: 1
          y: 2
          r: 10
          label: center/big

        - # Merge one map
          << : *CENTER
          r: 10
          label: center/big

        - # Merge multiple maps
          << : [ *CENTER, *BIG ]
          label: center/big

        - # Override
          << : [ *BIG, *LEFT, *SMALL ]
          x: 1
          label: center/big
    "};

    let mut value: Value = serde_yaml_ng::from_str(yaml).unwrap();
    value.apply_merge().unwrap();
    for i in 5..=7 {
        assert_eq!(value[4], value[i]);
    }
}

#[test]
fn test_debug() {
    let yaml = indoc! {"
        'Null': ~
        Bool: true
        Number: 1
        String: ...
        Sequence:
          - true
        EmptySequence: []
        EmptyMapping: {}
        Tagged: !tag true
    "};

    let value: Value = serde_yaml_ng::from_str(yaml).unwrap();
    let debug = format!("{:#?}", value);

    let expected = indoc! {r#"
        Mapping {
            "Null": Null,
            "Bool": Bool(true),
            "Number": Number(1),
            "String": String("..."),
            "Sequence": Sequence [
                Bool(true),
            ],
            "EmptySequence": Sequence [],
            "EmptyMapping": Mapping {},
            "Tagged": TaggedValue {
                tag: !tag,
                value: Bool(true),
            },
        }"#
    };

    assert_eq!(debug, expected);
}

#[test]
fn test_tagged() {
    #[derive(Serialize)]
    enum Enum {
        Variant(usize),
    }

    let value = serde_yaml_ng::to_value(Enum::Variant(0)).unwrap();

    let deserialized: serde_yaml_ng::Value = serde_yaml_ng::from_value(value.clone()).unwrap();
    assert_eq!(value, deserialized);

    let serialized = serde_yaml_ng::to_value(&value).unwrap();
    assert_eq!(value, serialized);
}

#[test]
#[cfg(feature = "128bit-support")]
fn test_value_with_i128() {
    use indoc::indoc;
    use serde_derive::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Demo {
        val_string: String,
        val_i128: i128,
        val: serde_yaml_ng::Value,
    }

    let yaml = indoc! {"
        val_string: '123'
        val_i128: -9223372036854776000
        val: -9223372036854776000
    "};

    let demo: Demo = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(demo.val_string, "123");
    assert_eq!(demo.val_i128, -9223372036854776000i128);

    match demo.val {
        serde_yaml_ng::Value::Number(ref n) => {
            assert_eq!(n.as_i128(), Some(-9223372036854776000i128));
        }
        _ => panic!("Expected Number value"),
    }

    let serialized = serde_yaml_ng::to_string(&demo).unwrap();
    let deserialized: Demo = serde_yaml_ng::from_str(&serialized).unwrap();
    assert_eq!(demo, deserialized);
}

#[test]
#[cfg(feature = "128bit-support")]
fn test_value_with_u128() {
    let yaml = "18446744073709551616";
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).unwrap();

    match value {
        serde_yaml_ng::Value::Number(ref n) => {
            assert!(n.is_u128());
            assert_eq!(n.as_u128(), Some(18446744073709551616u128));
        }
        _ => panic!("Expected Number value"),
    }

    let serialized = serde_yaml_ng::to_string(&value).unwrap();
    let deserialized: serde_yaml_ng::Value = serde_yaml_ng::from_str(&serialized).unwrap();
    assert_eq!(value, deserialized);
}

#[test]
#[cfg(feature = "128bit-support")]
fn test_i128_u128_extreme_values() {
    let yaml = "170141183460469231731687303715884105727";
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).unwrap();
    if let serde_yaml_ng::Value::Number(n) = value {
        assert_eq!(n.as_i128(), Some(i128::MAX));
    }

    let yaml = "-170141183460469231731687303715884105728";
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).unwrap();
    if let serde_yaml_ng::Value::Number(n) = value {
        assert_eq!(n.as_i128(), Some(i128::MIN));
    }

    let yaml = "340282366920938463463374607431768211455";
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).unwrap();
    if let serde_yaml_ng::Value::Number(n) = value {
        assert_eq!(n.as_u128(), Some(u128::MAX));
    }
}

#[test]
#[cfg(feature = "128bit-support")]
fn test_number_comparison_across_types() {
    let small = Number::from(42u64);
    let large = Number::from(42u128);
    assert_eq!(small, large);

    let small_neg = Number::from(-100i64);
    let large_neg = Number::from(-100i128);
    assert_eq!(small_neg, large_neg);

    let beyond_i64 = Number::from(i64::MIN as i128 - 1);
    let i64_min = Number::from(i64::MIN);
    assert!(beyond_i64 < i64_min);
}
