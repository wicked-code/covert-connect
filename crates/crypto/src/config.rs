use serde::{de::Error, Deserialize, Serialize, Deserializer, Serializer};
use std::ops::Range;

use super::{
    kdf::Kdf,
    cipher::CipherType
};

#[derive(Clone, Copy, PartialEq, Deserialize, Serialize, Debug)]
#[serde(remote = "Self")]
pub struct DataPadding {
    pub max: u16,
    pub rate: u8,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolConfig {
    /// master key
    pub key: String,

    /// key derivation function
    #[serde(default = "default_kdf")]
    pub kdf: Kdf,

    /// encryption type
    #[serde(default = "default_cipher")]
    pub cipher: CipherType,

    /// maximum delay before client connect to server (replay protection)
    /// should be the same for server and client
    /// 10000 (10 sec) default
    #[serde(default = "default_max_connect_delay")]
    pub max_connect_delay: u16,

    /// min a max header padding in bytes
    #[serde(default = "default_header_padding")]
    #[serde(deserialize_with = "range_from_human_readable")]
    #[serde(serialize_with = "range_to_human_readable")]
    pub header_padding: Range<u16>,

    /// rate and max value for data padding
    #[serde(default)]
    pub data_padding: DataPadding,

    /// encryption limit
    /// default is usize::MAX (encrypt all data)
    #[serde(default = "defaut_encryption_limit")]
    pub encryption_limit: usize,
}

fn default_max_connect_delay() -> u16 {
    10000
}

fn default_header_padding() -> Range<u16> {
    50..777
}

fn default_kdf() -> Kdf {
    Kdf::Argon2
}

fn default_cipher() -> CipherType {
    CipherType::Aes256Gcm
}

fn defaut_encryption_limit() -> usize {
    usize::MAX
}

fn range_to_human_readable<S>(value: &Range<u16>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer
{
    serializer.serialize_str(&(value.start.to_string() + ".." + &value.end.to_string()))
}

pub fn range_from_human_readable<'de, D>(deserializer: D) -> Result<Range<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let text: String = Deserialize::deserialize(deserializer)?;
    let mut parts = text.split("..");

    let start_str = parts.next()
        .ok_or_else(|| Error::custom(format!("range not found in \"{text}\"")))?;
    let end_str = parts.next()
        .ok_or_else(|| Error::custom(format!("range end not found in \"{text}\"")))?;
    if parts.next().is_some() {
        return Err(Error::custom(format!("only one range allowed: \"{text}\"")));
    }

    let start = start_str.parse::<u16>()
        .map_err(|err| Error::custom(format!("parse range \"{}\" err: {}", text, err)))?;
    let end = end_str.parse::<u16>()
        .map_err(|err| Error::custom(format!("parse range \"{}\" err: {}", text, err)))?;

    Ok(start..end)
}

impl DataPadding {
    pub fn needed(&self) -> bool {
        self.max > 0 && self.rate > 0
    }
}

impl Default for DataPadding {
    fn default() -> Self {
        Self {
            max: 255,
            rate: 20
        }
    }
}

impl<'de> Deserialize<'de> for DataPadding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let this = Self::deserialize(deserializer)?;
        if this.rate > 100 {
            return Err(Error::custom("Padding rate is to high. Max value is 100."));
        }
        Ok(this)
    }
}

impl Serialize for DataPadding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Self::serialize(self, serializer)
    }
}
