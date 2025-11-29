use crate::de;
use crate::error::{self, Error, ErrorImpl};
use serde::de::{Unexpected, Visitor};
use serde::{forward_to_deserialize_any, Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
use std::str::FromStr;

/// Represents a YAML number, whether integer or floating point.
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Number {
    n: N,
}

// "N" is a prefix of "NegInt"... this is a false positive.
// https://github.com/Manishearth/rust-clippy/issues/1241
#[allow(clippy::enum_variant_names)]
#[cfg_attr(not(feature = "arbitrary_precision"), derive(Copy))]
#[derive(Clone)]
enum N {
    /// Regular positive integers that fit in u64
    PosRegInt(u64),
    /// Regular negative integers that fit in i64 (always less than zero)
    NegRegInt(i64),

    #[cfg(feature = "128bit-support")]
    /// Large positive integers in range (u64::MAX, u128::MAX]
    PosLargeInt(u128),

    #[cfg(feature = "128bit-support")]
    /// Large negative integers in range [i128::MIN, i64::MIN)
    NegLargeInt(i128),

    #[cfg(feature = "arbitrary_precision")]
    /// Arbitrary precision positive integers as strings
    PosHugeInt(String),

    #[cfg(feature = "arbitrary_precision")]
    /// Arbitrary precision negative integers as strings (includes '-' prefix)
    NegHugeInt(String),

    /// May be infinite or NaN
    Float(f64),
}

impl Number {
    /// Returns true if the `Number` is an integer between `i64::MIN` and
    /// `i64::MAX`.
    ///
    /// For any Number on which `is_i64` returns true, `as_i64` is guaranteed to
    /// return the integer value.
    ///
    /// ```
    /// # fn main() -> serde_yaml_ng::Result<()> {
    /// let big = i64::MAX as u64 + 10;
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(r#"
    /// a: 64
    /// b: 9223372036854775817
    /// c: 256.0
    /// "#)?;
    ///
    /// assert!(v["a"].is_i64());
    ///
    /// // Greater than i64::MAX.
    /// assert!(!v["b"].is_i64());
    ///
    /// // Numbers with a decimal point are not considered integers.
    /// assert!(!v["c"].is_i64());
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub fn is_i64(&self) -> bool {
        match self.n {
            N::PosRegInt(v) => v <= i64::MAX as u64,
            N::NegRegInt(_) => true,
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(_) | N::NegLargeInt(_) => false,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => false,
            N::Float(_) => false,
        }
    }

    /// Returns true if the `Number` is an integer between zero and `u64::MAX`.
    ///
    /// For any Number on which `is_u64` returns true, `as_u64` is guaranteed to
    /// return the integer value.
    ///
    /// ```
    /// # fn main() -> serde_yaml_ng::Result<()> {
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(r#"
    /// a: 64
    /// b: -64
    /// c: 256.0
    /// "#)?;
    ///
    /// assert!(v["a"].is_u64());
    ///
    /// // Negative integer.
    /// assert!(!v["b"].is_u64());
    ///
    /// // Numbers with a decimal point are not considered integers.
    /// assert!(!v["c"].is_u64());
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn is_u64(&self) -> bool {
        match self.n {
            N::PosRegInt(_) => true,
            N::NegRegInt(_) => false,
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(_) | N::NegLargeInt(_) => false,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => false,
            N::Float(_) => false,
        }
    }

    /// Returns true if the `Number` can be represented by f64.
    ///
    /// For any Number on which `is_f64` returns true, `as_f64` is guaranteed to
    /// return the floating point value.
    ///
    /// Currently this function returns true if and only if both `is_i64` and
    /// `is_u64` return false but this is not a guarantee in the future.
    ///
    /// ```
    /// # fn main() -> serde_yaml_ng::Result<()> {
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(r#"
    /// a: 256.0
    /// b: 64
    /// c: -64
    /// "#)?;
    ///
    /// assert!(v["a"].is_f64());
    ///
    /// // Integers.
    /// assert!(!v["b"].is_f64());
    /// assert!(!v["c"].is_f64());
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn is_f64(&self) -> bool {
        match self.n {
            N::Float(_) => true,
            N::PosRegInt(_) | N::NegRegInt(_) => false,
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(_) | N::NegLargeInt(_) => false,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => false,
        }
    }

    /// If the `Number` is an integer, represent it as i64 if possible. Returns
    /// None otherwise.
    ///
    /// ```
    /// # fn main() -> serde_yaml_ng::Result<()> {
    /// let big = i64::MAX as u64 + 10;
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(r#"
    /// a: 64
    /// b: 9223372036854775817
    /// c: 256.0
    /// "#)?;
    ///
    /// assert_eq!(v["a"].as_i64(), Some(64));
    /// assert_eq!(v["b"].as_i64(), None);
    /// assert_eq!(v["c"].as_i64(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        match self.n {
            N::PosRegInt(n) => {
                if n <= i64::MAX as u64 {
                    Some(n as i64)
                } else {
                    None
                }
            }
            N::NegRegInt(n) => Some(n),
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(_) | N::NegLargeInt(_) => None,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => None,
            N::Float(_) => None,
        }
    }

    /// If the `Number` is an integer, represent it as u64 if possible. Returns
    /// None otherwise.
    ///
    /// ```
    /// # fn main() -> serde_yaml_ng::Result<()> {
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(r#"
    /// a: 64
    /// b: -64
    /// c: 256.0
    /// "#)?;
    ///
    /// assert_eq!(v["a"].as_u64(), Some(64));
    /// assert_eq!(v["b"].as_u64(), None);
    /// assert_eq!(v["c"].as_u64(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        match self.n {
            N::PosRegInt(n) => Some(n),
            N::NegRegInt(_) => None,
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(_) | N::NegLargeInt(_) => None,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => None,
            N::Float(_) => None,
        }
    }

    /// Represents the number as f64 if possible. Returns None otherwise.
    ///
    /// ```
    /// # fn main() -> serde_yaml_ng::Result<()> {
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(r#"
    /// a: 256.0
    /// b: 64
    /// c: -64
    /// "#)?;
    ///
    /// assert_eq!(v["a"].as_f64(), Some(256.0));
    /// assert_eq!(v["b"].as_f64(), Some(64.0));
    /// assert_eq!(v["c"].as_f64(), Some(-64.0));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// # fn main() -> serde_yaml_ng::Result<()> {
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(".inf")?;
    /// assert_eq!(v.as_f64(), Some(f64::INFINITY));
    ///
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str("-.inf")?;
    /// assert_eq!(v.as_f64(), Some(f64::NEG_INFINITY));
    ///
    /// let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(".nan")?;
    /// assert!(v.as_f64().unwrap().is_nan());
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        match &self.n {
            N::PosRegInt(n) => Some(*n as f64),
            N::NegRegInt(n) => Some(*n as f64),
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(n) => Some(*n as f64),
            #[cfg(feature = "128bit-support")]
            N::NegLargeInt(n) => Some(*n as f64),
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(s) => s.parse().ok(),
            #[cfg(feature = "arbitrary_precision")]
            N::NegHugeInt(s) => s.parse().ok(),
            N::Float(n) => Some(*n),
        }
    }

    /// Returns true if this value is NaN and false otherwise.
    ///
    /// ```
    /// # use serde_yaml_ng::Number;
    /// #
    /// assert!(!Number::from(256.0).is_nan());
    ///
    /// assert!(Number::from(f64::NAN).is_nan());
    ///
    /// assert!(!Number::from(f64::INFINITY).is_nan());
    ///
    /// assert!(!Number::from(f64::NEG_INFINITY).is_nan());
    ///
    /// assert!(!Number::from(1).is_nan());
    /// ```
    #[inline]
    pub fn is_nan(&self) -> bool {
        match self.n {
            N::PosRegInt(_) | N::NegRegInt(_) => false,
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(_) | N::NegLargeInt(_) => false,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => false,
            N::Float(f) => f.is_nan(),
        }
    }

    /// Returns true if this value is positive infinity or negative infinity and
    /// false otherwise.
    ///
    /// ```
    /// # use serde_yaml_ng::Number;
    /// #
    /// assert!(!Number::from(256.0).is_infinite());
    ///
    /// assert!(!Number::from(f64::NAN).is_infinite());
    ///
    /// assert!(Number::from(f64::INFINITY).is_infinite());
    ///
    /// assert!(Number::from(f64::NEG_INFINITY).is_infinite());
    ///
    /// assert!(!Number::from(1).is_infinite());
    /// ```
    #[inline]
    pub fn is_infinite(&self) -> bool {
        match self.n {
            N::PosRegInt(_) | N::NegRegInt(_) => false,
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(_) | N::NegLargeInt(_) => false,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => false,
            N::Float(f) => f.is_infinite(),
        }
    }

    /// Returns true if this number is neither infinite nor NaN.
    ///
    /// ```
    /// # use serde_yaml_ng::Number;
    /// #
    /// assert!(Number::from(256.0).is_finite());
    ///
    /// assert!(!Number::from(f64::NAN).is_finite());
    ///
    /// assert!(!Number::from(f64::INFINITY).is_finite());
    ///
    /// assert!(!Number::from(f64::NEG_INFINITY).is_finite());
    ///
    /// assert!(Number::from(1).is_finite());
    /// ```
    #[inline]
    pub fn is_finite(&self) -> bool {
        match self.n {
            N::PosRegInt(_) | N::NegRegInt(_) => true,
            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(_) | N::NegLargeInt(_) => true,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => true,
            N::Float(f) => f.is_finite(),
        }
    }

    /// Returns true if the `Number` is an integer between `i128::MIN` and `i128::MAX`.
    #[cfg(feature = "128bit-support")]
    pub fn is_i128(&self) -> bool {
        match self.n {
            N::PosRegInt(v) => v <= i128::MAX as u64,
            N::NegRegInt(_) => true,
            N::PosLargeInt(v) => v <= i128::MAX as u128,
            N::NegLargeInt(_) => true,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => false,
            N::Float(_) => false,
        }
    }

    /// Returns true if the `Number` is an integer between zero and `u128::MAX`.
    #[cfg(feature = "128bit-support")]
    pub fn is_u128(&self) -> bool {
        match self.n {
            N::PosRegInt(_) => true,
            N::NegRegInt(_) => false,
            N::PosLargeInt(_) => true,
            N::NegLargeInt(_) => false,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(_) | N::NegHugeInt(_) => false,
            N::Float(_) => false,
        }
    }

    /// If the `Number` is an integer, represent it as i128 if possible.
    #[cfg(feature = "128bit-support")]
    pub fn as_i128(&self) -> Option<i128> {
        match &self.n {
            N::PosRegInt(n) => Some(*n as i128),
            N::NegRegInt(n) => Some(*n as i128),
            N::PosLargeInt(n) => {
                if *n <= i128::MAX as u128 {
                    Some(*n as i128)
                } else {
                    None
                }
            }
            N::NegLargeInt(n) => Some(*n),
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(s) => s.parse().ok(),
            #[cfg(feature = "arbitrary_precision")]
            N::NegHugeInt(s) => s.parse().ok(),
            N::Float(_) => None,
        }
    }

    /// If the `Number` is an integer, represent it as u128 if possible.
    #[cfg(feature = "128bit-support")]
    pub fn as_u128(&self) -> Option<u128> {
        match &self.n {
            N::PosRegInt(n) => Some(*n as u128),
            N::NegRegInt(_) => None,
            N::PosLargeInt(n) => Some(*n),
            N::NegLargeInt(_) => None,
            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(s) => s.parse().ok(),
            #[cfg(feature = "arbitrary_precision")]
            N::NegHugeInt(_) => None,
            N::Float(_) => None,
        }
    }

    /// Returns true if the `Number` is stored as an arbitrary precision string.
    #[cfg(feature = "arbitrary_precision")]
    pub fn is_arbitrary_precision(&self) -> bool {
        matches!(self.n, N::PosHugeInt(_) | N::NegHugeInt(_))
    }

    /// If the `Number` is stored as an arbitrary precision string, return the string representation.
    #[cfg(feature = "arbitrary_precision")]
    pub fn as_arbitrary_precision(&self) -> Option<&str> {
        match &self.n {
            N::PosHugeInt(s) | N::NegHugeInt(s) => Some(s),
            _ => None,
        }
    }
}

impl Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match &self.n {
            N::PosRegInt(i) => formatter.write_str(itoa::Buffer::new().format(*i)),
            N::NegRegInt(i) => formatter.write_str(itoa::Buffer::new().format(*i)),

            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(i) => formatter.write_str(itoa::Buffer::new().format(*i)),
            #[cfg(feature = "128bit-support")]
            N::NegLargeInt(i) => formatter.write_str(itoa::Buffer::new().format(*i)),

            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(s) => formatter.write_str(s),
            #[cfg(feature = "arbitrary_precision")]
            N::NegHugeInt(s) => formatter.write_str(s),

            N::Float(f) if f.is_nan() => formatter.write_str(".nan"),
            N::Float(f) if f.is_infinite() => {
                if f.is_sign_negative() {
                    formatter.write_str("-.inf")
                } else {
                    formatter.write_str(".inf")
                }
            }
            N::Float(f) => formatter.write_str(ryu::Buffer::new().format_finite(*f)),
        }
    }
}

impl FromStr for Number {
    type Err = Error;

    fn from_str(repr: &str) -> Result<Self, Self::Err> {
        if let Ok(result) = de::visit_int(NumberVisitor, repr) {
            return result;
        }
        if !de::digits_but_not_number(repr) {
            if let Some(float) = de::parse_f64(repr) {
                return Ok(float.into());
            }
        }

        #[cfg(feature = "arbitrary_precision")]
        {
            if de::is_valid_integer_string(repr) {
                return if repr.starts_with('-') {
                    Ok(Number {
                        n: N::NegHugeInt(repr.to_string()),
                    })
                } else {
                    Ok(Number {
                        n: N::PosHugeInt(repr.strip_prefix('+').unwrap_or(repr).to_string()),
                    })
                };
            }
        }

        Err(error::new(ErrorImpl::FailedToParseNumber))
    }
}

impl PartialEq for N {
    fn eq(&self, other: &N) -> bool {
        match (self, other) {
            (N::PosRegInt(a), N::PosRegInt(b)) => a == b,
            (N::NegRegInt(a), N::NegRegInt(b)) => a == b,

            #[cfg(feature = "128bit-support")]
            (N::PosLargeInt(a), N::PosLargeInt(b)) => a == b,
            #[cfg(feature = "128bit-support")]
            (N::NegLargeInt(a), N::NegLargeInt(b)) => a == b,

            // Cross-type regular/large int comparisons
            #[cfg(feature = "128bit-support")]
            (N::PosRegInt(a), N::PosLargeInt(b)) | (N::PosLargeInt(b), N::PosRegInt(a)) => {
                *a as u128 == *b
            }
            #[cfg(feature = "128bit-support")]
            (N::NegRegInt(a), N::NegLargeInt(b)) | (N::NegLargeInt(b), N::NegRegInt(a)) => {
                *a as i128 == *b
            }

            #[cfg(feature = "arbitrary_precision")]
            (N::PosHugeInt(a), N::PosHugeInt(b)) => a == b,
            #[cfg(feature = "arbitrary_precision")]
            (N::NegHugeInt(a), N::NegHugeInt(b)) => a == b,

            // Arbitrary precision comparisons with other types
            #[cfg(feature = "arbitrary_precision")]
            (N::PosHugeInt(s), other) | (other, N::PosHugeInt(s)) => {
                if let Ok(val) = s.parse::<u128>() {
                    // Try comparing as u128
                    match other {
                        N::PosRegInt(b) => val == *b as u128,
                        #[cfg(feature = "128bit-support")]
                        N::PosLargeInt(b) => val == *b,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            #[cfg(feature = "arbitrary_precision")]
            (N::NegHugeInt(s), other) | (other, N::NegHugeInt(s)) => {
                if let Ok(val) = s.parse::<i128>() {
                    match other {
                        N::NegRegInt(b) => val == *b as i128,
                        #[cfg(feature = "128bit-support")]
                        N::NegLargeInt(b) => val == *b,
                        _ => false,
                    }
                } else {
                    false
                }
            }

            (N::Float(a), N::Float(b)) => {
                if a.is_nan() && b.is_nan() {
                    // YAML only has one NaN;
                    // the bit representation isn't preserved
                    true
                } else {
                    a == b
                }
            }
            _ => false,
        }
    }
}

impl PartialOrd for N {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (N::Float(a), N::Float(b)) => {
                if a.is_nan() && b.is_nan() {
                    // YAML only has one NaN
                    Some(Ordering::Equal)
                } else {
                    a.partial_cmp(b)
                }
            }
            _ => Some(self.total_cmp(other)),
        }
    }
}

impl N {
    fn total_cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Same-type comparisons
            (N::PosRegInt(a), N::PosRegInt(b)) => a.cmp(b),
            (N::NegRegInt(a), N::NegRegInt(b)) => a.cmp(b),

            #[cfg(feature = "128bit-support")]
            (N::PosLargeInt(a), N::PosLargeInt(b)) => a.cmp(b),
            #[cfg(feature = "128bit-support")]
            (N::NegLargeInt(a), N::NegLargeInt(b)) => a.cmp(b),

            #[cfg(feature = "arbitrary_precision")]
            (N::PosHugeInt(a), N::PosHugeInt(b)) => {
                // String comparison for huge ints - compare by length first, then lexically
                match a.len().cmp(&b.len()) {
                    Ordering::Equal => a.cmp(b),
                    other => other,
                }
            }
            #[cfg(feature = "arbitrary_precision")]
            (N::NegHugeInt(a), N::NegHugeInt(b)) => {
                // For negative, reverse the comparison (longer = more negative)
                match b.len().cmp(&a.len()) {
                    Ordering::Equal => b.cmp(a),
                    other => other,
                }
            }

            // Negative vs positive - negative always less
            (N::NegRegInt(_), N::PosRegInt(_)) => Ordering::Less,
            (N::PosRegInt(_), N::NegRegInt(_)) => Ordering::Greater,

            #[cfg(feature = "128bit-support")]
            (N::NegRegInt(_), N::PosLargeInt(_))
            | (N::NegLargeInt(_), N::PosRegInt(_))
            | (N::NegLargeInt(_), N::PosLargeInt(_)) => Ordering::Less,

            #[cfg(feature = "128bit-support")]
            (N::PosRegInt(_), N::NegLargeInt(_))
            | (N::PosLargeInt(_), N::NegRegInt(_))
            | (N::PosLargeInt(_), N::NegLargeInt(_)) => Ordering::Greater,

            #[cfg(all(feature = "arbitrary_precision", not(feature = "128bit-support")))]
            (N::NegHugeInt(_), N::PosRegInt(_)) | (N::PosRegInt(_), N::NegHugeInt(_)) => {
                if matches!(self, N::NegHugeInt(_)) {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            #[cfg(all(feature = "arbitrary_precision", not(feature = "128bit-support")))]
            (N::NegHugeInt(_), N::PosHugeInt(_)) | (N::PosHugeInt(_), N::NegHugeInt(_)) => {
                if matches!(self, N::NegHugeInt(_)) {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            #[cfg(all(feature = "arbitrary_precision", not(feature = "128bit-support")))]
            (N::PosRegInt(_), N::NegHugeInt(_)) | (N::NegRegInt(_), N::PosHugeInt(_)) => {
                if matches!(self, N::NegRegInt(_)) {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }

            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::NegHugeInt(_), N::PosRegInt(_)) | (N::NegHugeInt(_), N::PosHugeInt(_)) => {
                Ordering::Less
            }
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::PosHugeInt(_), N::NegRegInt(_)) | (N::PosHugeInt(_), N::NegHugeInt(_)) => {
                Ordering::Greater
            }
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::PosRegInt(_), N::NegHugeInt(_)) => Ordering::Greater,
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::NegRegInt(_), N::PosHugeInt(_)) => Ordering::Less,

            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::NegHugeInt(_), N::PosLargeInt(_)) => Ordering::Less,
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::PosHugeInt(_), N::NegLargeInt(_)) => Ordering::Greater,
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::NegLargeInt(_), N::PosHugeInt(_)) => Ordering::Less,
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::PosLargeInt(_), N::NegHugeInt(_)) => Ordering::Greater,

            // Cross-type within same sign
            #[cfg(feature = "128bit-support")]
            (N::PosRegInt(a), N::PosLargeInt(b)) => (*a as u128).cmp(b),
            #[cfg(feature = "128bit-support")]
            (N::PosLargeInt(a), N::PosRegInt(b)) => a.cmp(&(*b as u128)),

            #[cfg(feature = "128bit-support")]
            (N::NegRegInt(a), N::NegLargeInt(b)) => (*a as i128).cmp(b),
            #[cfg(feature = "128bit-support")]
            (N::NegLargeInt(a), N::NegRegInt(b)) => a.cmp(&(*b as i128)),

            // Arbitrary precision with regular/large types
            #[cfg(all(feature = "arbitrary_precision", not(feature = "128bit-support")))]
            (N::PosHugeInt(s), N::PosRegInt(b)) => {
                match s.parse::<u64>() {
                    Ok(a) => a.cmp(b),
                    Err(_) => Ordering::Greater, // Huge int is bigger than u64
                }
            }
            #[cfg(all(feature = "arbitrary_precision", not(feature = "128bit-support")))]
            (N::PosRegInt(a), N::PosHugeInt(s)) => match s.parse::<u64>() {
                Ok(b) => a.cmp(&b),
                Err(_) => Ordering::Less,
            },

            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::PosHugeInt(s), N::PosRegInt(b)) => match s.parse::<u128>() {
                Ok(a) => a.cmp(&(*b as u128)),
                Err(_) => Ordering::Greater,
            },
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::PosRegInt(a), N::PosHugeInt(s)) => match s.parse::<u128>() {
                Ok(b) => (*a as u128).cmp(&b),
                Err(_) => Ordering::Less,
            },

            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::PosHugeInt(s), N::PosLargeInt(b)) => match s.parse::<u128>() {
                Ok(a) => a.cmp(b),
                Err(_) => Ordering::Greater,
            },
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::PosLargeInt(a), N::PosHugeInt(s)) => match s.parse::<u128>() {
                Ok(b) => a.cmp(&b),
                Err(_) => Ordering::Less,
            },

            // Similar for negative huge ints...
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::NegHugeInt(s), N::NegRegInt(b)) => {
                match s.parse::<i128>() {
                    Ok(a) => a.cmp(&(*b as i128)),
                    Err(_) => Ordering::Less, // More negative
                }
            }
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::NegRegInt(a), N::NegHugeInt(s)) => match s.parse::<i128>() {
                Ok(b) => (*a as i128).cmp(&b),
                Err(_) => Ordering::Greater,
            },

            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::NegHugeInt(s), N::NegLargeInt(b)) => match s.parse::<i128>() {
                Ok(a) => a.cmp(b),
                Err(_) => Ordering::Less,
            },
            #[cfg(all(feature = "arbitrary_precision", feature = "128bit-support"))]
            (N::NegLargeInt(a), N::NegHugeInt(s)) => match s.parse::<i128>() {
                Ok(b) => a.cmp(&b),
                Err(_) => Ordering::Greater,
            },

            // Float comparisons - integers below floats
            (N::Float(a), N::Float(b)) => a.partial_cmp(b).unwrap_or_else(|| {
                if !a.is_nan() {
                    Ordering::Less
                } else if !b.is_nan() {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            }),
            (_, N::Float(_)) => Ordering::Less,
            (N::Float(_), _) => Ordering::Greater,
        }
    }
}

impl Number {
    pub(crate) fn total_cmp(&self, other: &Self) -> Ordering {
        self.n.total_cmp(&other.n)
    }
}

impl Serialize for Number {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.n {
            N::PosRegInt(i) => serializer.serialize_u64(*i),
            N::NegRegInt(i) => serializer.serialize_i64(*i),

            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(i) => serializer.serialize_u128(*i),
            #[cfg(feature = "128bit-support")]
            N::NegLargeInt(i) => serializer.serialize_i128(*i),

            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(s) | N::NegHugeInt(s) => {
                // Serialize as string since serde doesn't have arbitrary precision integers
                serializer.serialize_str(s)
            }

            N::Float(f) => serializer.serialize_f64(*f),
        }
    }
}

struct NumberVisitor;

impl Visitor<'_> for NumberVisitor {
    type Value = Number;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a number")
    }

    #[inline]
    fn visit_i64<E>(self, value: i64) -> Result<Number, E> {
        Ok(value.into())
    }

    #[inline]
    fn visit_u64<E>(self, value: u64) -> Result<Number, E> {
        Ok(value.into())
    }

    #[cfg(feature = "128bit-support")]
    #[inline]
    fn visit_i128<E>(self, value: i128) -> Result<Number, E> {
        Ok(value.into())
    }

    #[cfg(feature = "128bit-support")]
    #[inline]
    fn visit_u128<E>(self, value: u128) -> Result<Number, E> {
        Ok(value.into())
    }

    #[inline]
    fn visit_f64<E>(self, value: f64) -> Result<Number, E> {
        Ok(value.into())
    }

    #[cfg(feature = "arbitrary_precision")]
    fn visit_str<E>(self, s: &str) -> Result<Number, E>
    where
        E: serde::de::Error,
    {
        if s.starts_with('-') {
            Ok(Number {
                n: N::NegHugeInt(s.to_string()),
            })
        } else {
            Ok(Number {
                n: N::PosHugeInt(s.to_string()),
            })
        }
    }
}

impl<'de> Deserialize<'de> for Number {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Number, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(NumberVisitor)
    }
}

impl<'de> Deserializer<'de> for Number {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match self.n {
            N::PosRegInt(i) => visitor.visit_u64(i),
            N::NegRegInt(i) => visitor.visit_i64(i),

            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(i) => visitor.visit_u128(i),
            #[cfg(feature = "128bit-support")]
            N::NegLargeInt(i) => visitor.visit_i128(i),

            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(ref s) | N::NegHugeInt(ref s) => visitor.visit_str(s),

            N::Float(f) => visitor.visit_f64(f),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de> Deserializer<'de> for &Number {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        match &self.n {
            N::PosRegInt(i) => visitor.visit_u64(*i),
            N::NegRegInt(i) => visitor.visit_i64(*i),

            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(i) => visitor.visit_u128(*i),
            #[cfg(feature = "128bit-support")]
            N::NegLargeInt(i) => visitor.visit_i128(*i),

            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(s) | N::NegHugeInt(s) => visitor.visit_str(s),

            N::Float(f) => visitor.visit_f64(*f),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

macro_rules! from_signed {
    ($($signed_ty:ident)*) => {
        $(
            impl From<$signed_ty> for Number {
                #[inline]
                #[allow(clippy::cast_sign_loss)]
                fn from(i: $signed_ty) -> Self {
                    if i < 0 {
                        Number { n: N::NegRegInt(i as i64) }
                    } else {
                        Number { n: N::PosRegInt(i as u64) }
                    }
                }
            }
        )*
    };
}

macro_rules! from_unsigned {
    ($($unsigned_ty:ident)*) => {
        $(
            impl From<$unsigned_ty> for Number {
                #[inline]
                fn from(u: $unsigned_ty) -> Self {
                    Number { n: N::PosRegInt(u as u64) }
                }
            }
        )*
    };
}

from_signed!(i8 i16 i32 i64 isize);
from_unsigned!(u8 u16 u32 u64 usize);

#[cfg(feature = "128bit-support")]
impl From<i128> for Number {
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    fn from(i: i128) -> Self {
        if let Ok(i64_val) = i64::try_from(i) {
            if i64_val < 0 {
                Number {
                    n: N::NegRegInt(i64_val),
                }
            } else {
                Number {
                    n: N::PosRegInt(i64_val as u64),
                }
            }
        } else {
            Number {
                n: N::NegLargeInt(i),
            }
        }
    }
}

#[cfg(feature = "128bit-support")]
impl From<u128> for Number {
    #[inline]
    fn from(u: u128) -> Self {
        if let Ok(u64_val) = u64::try_from(u) {
            Number {
                n: N::PosRegInt(u64_val),
            }
        } else {
            Number {
                n: N::PosLargeInt(u),
            }
        }
    }
}

impl From<f32> for Number {
    fn from(f: f32) -> Self {
        Number::from(f as f64)
    }
}

impl From<f64> for Number {
    fn from(mut f: f64) -> Self {
        if f.is_nan() {
            // Destroy NaN sign, signaling, and payload. YAML only has one NaN.
            f = f64::NAN.copysign(1.0);
        }
        Number { n: N::Float(f) }
    }
}

// This is fine, because we don't _really_ implement hash for floats
// all other hash functions should work as expected
#[allow(clippy::derived_hash_with_manual_eq)]
impl Hash for Number {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.n {
            N::Float(_) => {
                // you should feel bad for using f64 as a map key
                3.hash(state);
            }
            N::PosRegInt(u) => u.hash(state),
            N::NegRegInt(i) => i.hash(state),

            #[cfg(feature = "128bit-support")]
            N::PosLargeInt(u) => u.hash(state),
            #[cfg(feature = "128bit-support")]
            N::NegLargeInt(i) => i.hash(state),

            #[cfg(feature = "arbitrary_precision")]
            N::PosHugeInt(s) | N::NegHugeInt(s) => s.hash(state),
        }
    }
}

pub(crate) fn unexpected(number: &Number) -> Unexpected<'_> {
    match &number.n {
        N::PosRegInt(u) => Unexpected::Unsigned(*u),
        N::NegRegInt(i) => Unexpected::Signed(*i),

        #[cfg(feature = "128bit-support")]
        N::PosLargeInt(_) | N::NegLargeInt(_) => Unexpected::Other("large integer (i128/u128)"),

        #[cfg(feature = "arbitrary_precision")]
        N::PosHugeInt(s) | N::NegHugeInt(s) => Unexpected::Str(s),

        N::Float(f) => Unexpected::Float(*f),
    }
}
