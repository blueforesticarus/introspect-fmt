//! Owned argument values. Opaque arguments are never owned directly: their
//! requested trait output is captured as text in a rendered slot instead.
use crate::{CaptureSink, Slots, Value};

// Keep exact primitive types here: widening signed integers changes hex output,
// and widening f32 changes decimal/debug formatting even when its bits survive.
macro_rules! owned_values {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum OwnedValue { $($variant($ty),)* Text(String) }
        impl OwnedValue {
            pub fn as_value(&self) -> Value<'_> {
                match self {
                    $(Self::$variant(value) => Value::$variant(value),)*
                    Self::Text(value) => Value::Str(value),
                }
            }
            /// `None` for opaque values, which have no owned representation.
            pub fn from_value(value: &Value<'_>) -> Option<Self> {
                Some(match value {
                    $(Value::$variant(value) => Self::$variant(**value),)*
                    Value::Str(value) => Self::Text((*value).into()),
                    Value::Opaque(_) => return None,
                })
            }
        }
    };
}
owned_values! {
    Bool(bool), Char(char), I8(i8), I16(i16), I32(i32), I64(i64), I128(i128), Isize(isize),
    U8(u8), U16(u16), U32(u32), U64(u64), U128(u128), Usize(usize), F32(f32), F64(f64),
}

impl CaptureSink for Vec<OwnedValue> {
    fn value(&mut self, value: &Value<'_>) {
        self.extend(OwnedValue::from_value(value));
    }
    fn text(&mut self) -> &mut String {
        self.push(OwnedValue::Text(String::new()));
        let Some(OwnedValue::Text(text)) = self.last_mut() else {
            unreachable!()
        };
        text
    }
}

impl Slots for [OwnedValue] {
    fn slot(&self, index: usize) -> Option<Value<'_>> {
        self.get(index).map(OwnedValue::as_value)
    }
}
