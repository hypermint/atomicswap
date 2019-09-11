use hmcdk::prelude::*;
use hmcdk::error;

pub fn deserialize<'a, T>(b: &'a [u8]) -> Result<T, Error>
where
    T: serde::Deserialize<'a>,
{
    match serde_json::from_slice(b) {
        Ok(v) => Ok(v),
        Err(_m) => Err(error::from_str(format!("failed to deserialize: {:?}", b))),
    }
}

pub fn serialize<T: ?Sized>(value: &T) -> Result<Vec<u8>, Error>
where
    T: serde::Serialize,
{
    match serde_json::to_vec(value) {
        Ok(v) => Ok(v),
        Err(_m) => Err(error::from_str("failed to serialize")),
    }
}
