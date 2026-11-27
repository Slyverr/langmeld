//! Compiled languages as bytes, so that a program can start without reading or
//! checking a grammar.

use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt;

use serde::Serialize;

/// The first bytes of a written language.
const MAGIC: &[u8] = b"LMGR";
/// The version of the library, which decides what the bytes mean.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why a language could not be written or read back.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EmbedError {
    /// The bytes were not written from a language.
    NotALanguage,
    /// The bytes were written by this other version of Langmeld.
    OtherVersion(String),
    /// The bytes changed after they were written.
    Damaged,
    /// The language has a custom rule of this name, and no code for it was given.
    MissingRule(String),
    /// The language has an external token of this name, and no code for it was given.
    MissingToken(String),
    /// The language could not be written.
    Write(String),
}

impl fmt::Display for EmbedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotALanguage => write!(f, "these bytes are not a compiled language"),
            Self::OtherVersion(other) => write!(
                f,
                "the language was compiled by Langmeld {other}, and this is {VERSION}; \
                 compile it again"
            ),
            Self::Damaged => write!(f, "the compiled language is damaged; compile it again"),
            Self::MissingRule(name) => write!(f, "custom rule `{name}` has no code"),
            Self::MissingToken(name) => write!(f, "external token `{name}` has no code"),
            Self::Write(reason) => write!(f, "the language cannot be written: {reason}"),
        }
    }
}

impl Error for EmbedError {}

/// FNV-1a, which notices accidental changes.
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Puts the header in front of a written language.
pub(crate) fn wrap(payload: &[u8]) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    // Version strings are a few characters long.
    bytes.push(u8::try_from(VERSION.len()).unwrap_or(u8::MAX));
    bytes.extend_from_slice(VERSION.as_bytes());
    bytes.extend_from_slice(&checksum(payload).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

/// The written language after a header that fits it.
pub(crate) fn unwrap(bytes: &[u8]) -> Result<&[u8], EmbedError> {
    let rest = bytes.strip_prefix(MAGIC).ok_or(EmbedError::NotALanguage)?;
    let (&length, rest) = rest.split_first().ok_or(EmbedError::Damaged)?;
    let (version, rest) = rest
        .split_at_checked(usize::from(length))
        .ok_or(EmbedError::Damaged)?;
    if version != VERSION.as_bytes() {
        let other = String::from_utf8_lossy(version).into_owned();
        return Err(EmbedError::OtherVersion(other));
    }
    let (written, payload) = rest.split_first_chunk().ok_or(EmbedError::Damaged)?;
    if u64::from_le_bytes(*written) != checksum(payload) {
        return Err(EmbedError::Damaged);
    }
    Ok(payload)
}

/// Writes a map in the order of its keys, so that the same language gives the same
/// bytes.
pub(crate) mod sorted {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer, V: Serialize>(
        map: &HashMap<String, V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_map(map.iter().collect::<BTreeMap<_, _>>())
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<HashMap<String, V>, D::Error> {
        HashMap::deserialize(deserializer)
    }
}

/// Custom rules are written as nothing; reading them gives a stand-in that the real
/// code replaces.
pub(crate) mod rule_code {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serializer};

    use crate::Parser;
    use crate::grammar::ParseFn;

    pub(crate) fn serialize<S: Serializer>(
        _: &Arc<ParseFn>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit()
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<ParseFn>, D::Error> {
        <()>::deserialize(deserializer)?;
        Ok(Arc::new(|_: &mut Parser<'_>| {}))
    }
}

/// External tokens, like custom rules.
pub(crate) mod token_code {
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serializer};

    use crate::LexCursor;
    use crate::lexer::ExternalFn;

    pub(crate) fn serialize<S: Serializer>(
        _: &Arc<ExternalFn>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit()
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<ExternalFn>, D::Error> {
        <()>::deserialize(deserializer)?;
        Ok(Arc::new(|_: &mut LexCursor<'_>| None))
    }
}

/// The automaton of a lexer, in its own little-endian form.
pub(crate) mod automaton {
    use regex_automata::dfa::dense::DFA;
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(crate) fn serialize<S: Serializer>(
        automaton: &Option<DFA<Vec<u32>>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        automaton
            .as_ref()
            .map(|automaton| {
                let (bytes, padding) = automaton.to_bytes_little_endian();
                bytes.get(padding..).unwrap_or_default().to_vec()
            })
            .serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<DFA<Vec<u32>>>, D::Error> {
        let Some(bytes) = Option::<Vec<u8>>::deserialize(deserializer)? else {
            return Ok(None);
        };
        // The automaton is read from memory aligned for its numbers.
        let mut buffer = vec![0u8; bytes.len() + 4];
        let start = buffer.as_ptr().align_offset(4).min(4);
        let place = buffer
            .get_mut(start..start + bytes.len())
            .ok_or_else(|| D::Error::custom("no room for the automaton"))?;
        place.copy_from_slice(&bytes);
        let (automaton, _) = DFA::from_bytes(place).map_err(D::Error::custom)?;
        Ok(Some(automaton.to_owned()))
    }
}
