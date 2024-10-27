#![allow(missing_docs)]

use crate::Result;
use roxmltree::Node;
use rupnp::utils::find_node_attribute;
pub use rupnp::utils::find_root as find_root_node;
use std::borrow::Cow;

#[doc(hidden)]
#[macro_export]
macro_rules! args {
    ( $( $var:literal: $e:expr ),* ) => { &{
        let mut s = String::new();
        $(
            s.push_str(concat!("<", $var, ">"));
            s.push_str(&$e.to_string());
            s.push_str(concat!("</", $var, ">"));
        )*
        s
    } }
}

pub(crate) trait HashMapExt {
    fn extract(&mut self, key: &str) -> Result<String>;
}
impl HashMapExt for std::collections::HashMap<String, String> {
    fn extract(&mut self, key: &str) -> Result<String> {
        self.remove(key).ok_or_else(|| {
            rupnp::Error::XmlMissingElement("UPnP Response".to_string(), key.to_string()).into()
        })
    }
}

pub(crate) fn seconds_to_str(seconds_total: i64) -> String {
    let sign = if seconds_total < 0 { "-" } else { "" };
    let seconds_total = seconds_total.abs();

    let seconds = seconds_total % 60;
    let minutes = (seconds_total / 60) % 60;
    let hours = seconds_total / 3600;

    format!("{}{:02}:{:02}:{:02}", sign, hours, minutes, seconds)
}
pub(crate) fn seconds_from_str(s: &str) -> Result<u32> {
    let opt = (|| {
        let mut split = s.splitn(3, ':');
        let hours = split.next()?.parse::<u32>().ok()?;
        let minutes = split.next()?.parse::<u32>().ok()?;
        let seconds = split.next()?.parse::<u32>().ok()?;

        Some(hours * 3600 + minutes * 60 + seconds)
    })();

    opt.ok_or(rupnp::Error::ParseError("invalid duration").into())
}

pub(crate) fn parse_bool(s: String) -> Result<bool> {
    match s.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(rupnp::Error::ParseError("bool was neither `0` nor `1`").into()),
    }
}

pub fn try_find_node_attribute<'n, 'd: 'n>(node: Node<'d, 'n>, attr: &str) -> Result<&'n str> {
    find_node_attribute(node, attr).ok_or_else(|| {
        rupnp::Error::XmlMissingElement(node.tag_name().name().to_string(), attr.to_string()).into()
    })
}

// Functions for escaping XML special characters copied from xml-rs crate

// The MIT License (MIT)
// Copyright (c) 2014 Vladimir Matveev
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

enum Value {
    Char(char),
    Str(&'static str),
}

impl Value {
    fn dispatch_for_pcdata(c: char) -> Value {
        match c {
            '<' => Value::Str("&lt;"),
            '&' => Value::Str("&amp;"),
            _ => Value::Char(c),
        }
    }
}

enum Process<'a> {
    Borrowed(&'a str),
    Owned(String),
}

impl<'a> Process<'a> {
    fn process(&mut self, (i, next): (usize, Value)) {
        match next {
            Value::Str(s) => match *self {
                Process::Owned(ref mut o) => o.push_str(s),
                Process::Borrowed(b) => {
                    let mut r = String::with_capacity(b.len() + s.len());
                    r.push_str(&b[..i]);
                    r.push_str(s);
                    *self = Process::Owned(r);
                }
            },
            Value::Char(c) => match *self {
                Process::Borrowed(_) => {}
                Process::Owned(ref mut o) => o.push(c),
            },
        }
    }

    fn into_result(self) -> Cow<'a, str> {
        match self {
            Process::Borrowed(b) => Cow::Borrowed(b),
            Process::Owned(o) => Cow::Owned(o),
        }
    }
}

impl<'a> Extend<(usize, Value)> for Process<'a> {
    fn extend<I: IntoIterator<Item = (usize, Value)>>(&mut self, it: I) {
        for v in it.into_iter() {
            self.process(v);
        }
    }
}

fn escape_str(s: &str, dispatch: fn(char) -> Value) -> Cow<'_, str> {
    let mut p = Process::Borrowed(s);
    p.extend(s.char_indices().map(|(ind, c)| (ind, dispatch(c))));
    p.into_result()
}

/// Performs escaping of common XML characters inside PCDATA.
///
/// This function replaces several important markup characters with their
/// entity equivalents:
///
/// * `<` → `&lt;`
/// * `&` → `&amp;`
///
/// The resulting string is safe to use inside PCDATA sections but NOT inside attribute values.
///
/// Does not perform allocations if the given string does not contain escapable characters.
#[inline]
pub fn escape_str_pcdata(s: &str) -> Cow<'_, str> {
    escape_str(s, Value::dispatch_for_pcdata)
}

#[cfg(test)]
mod tests {
    use super::escape_str_pcdata;

    // TODO: add more tests

    #[test]
    fn test_escape_multibyte_code_points() {
        assert_eq!(escape_str_pcdata("☃<"), "☃&lt;");
    }
}
