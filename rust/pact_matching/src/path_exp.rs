use std::convert::TryFrom;
use std::hash::Hash;
use std::hash::Hasher;
use std::iter::Peekable;

use anyhow::anyhow;
use log::trace;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathToken {
    Root,
    Field(String),
    Index(usize),
    Star,
    StarIndex
}

fn matches_token(path_fragment: &str, path_token: &PathToken) -> usize {
  match path_token {
    PathToken::Root if path_fragment == "$" => 2,
    PathToken::Field(name) if path_fragment == name => 2,
    PathToken::Index(index) => match path_fragment.parse::<usize>() {
      Ok(i) if *index == i => 2,
      _ => 0
    },
    PathToken::StarIndex => match path_fragment.parse::<usize>() {
      Ok(_) => 1,
      _ => 0
    },
    PathToken::Star => 1,
    _ => 0
  }
}

#[serde(try_from = "String")]
#[serde(into = "String")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JSONPath {
    path_tokens: Vec<PathToken>,
    orig_string: String,
}

impl JSONPath {
  pub fn new(path: impl Into<String>) -> anyhow::Result<Self> {
    let path = path.into();
    let path_tokens = parse_path_exp(&path)
      .map_err(|e| anyhow!(e))?;
    Ok(Self {
      path_tokens,
      orig_string: path,
    })
  }

  // Convenient infallible construction for tests
  // where the expression is statically known.
  pub fn new_unwrap(path: &'static str) -> Self {
    Self::new(path).unwrap()
  }

  pub fn empty() -> Self {
    Self::new_unwrap("")
  }

  pub fn tokens(&self) -> &Vec<PathToken> {
    &self.path_tokens
  }

  pub fn orig_string(&self) -> &str {
    &self.orig_string
  }

  pub fn len(&self) -> usize {
    self.path_tokens.len()
  }

  // TODO test
  pub fn is_wildcard(&self) -> bool {
    self.path_tokens.last() == Some(&PathToken::Star)
  }

  pub fn calc_path_weight(&self, path: &[&str]) -> (usize, usize) {
    trace!("Calculating weight for path tokens '{:?}' and path '{:?}'",
           self.path_tokens, path);
    let weight = {
      if path.len() >= self.len() {
        (
          self.path_tokens.iter().zip(path.iter())
          .fold(1, |acc, (token, fragment)| acc * matches_token(fragment, token)),
          self.len()
        )
      } else {
        (0, self.len())
      }
    };
    trace!("Calculated weight {:?} for path '{}' and '{:?}'",
           weight, self.orig_string, path);
    weight
  }
}

impl From<JSONPath> for String {
  fn from(json_path: JSONPath) -> String {
    json_path.orig_string
  }
}

impl TryFrom<String> for JSONPath {
  type Error = anyhow::Error;

  fn try_from(path: String) -> Result<Self, Self::Error> {
    JSONPath::new(path)
  }
}

impl Hash for JSONPath {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.orig_string.hash(state);
  }
}

fn peek<I>(chars: &mut Peekable<I>) -> Option<(usize, char)> where I: Iterator<Item = (usize, char)> {
  chars.peek().map(|tup| (tup.0.clone(), tup.1.clone()))
}

fn is_identifier_char(ch: char) -> bool {
  ch.is_alphabetic() || ch.is_numeric() || ch == '_' || ch == '-' || ch == ':' || ch == '#' || ch == '@'
}

// identifier -> a-zA-Z0-9+
fn identifier<I>(ch: char, chars: &mut Peekable<I>, tokens: &mut Vec<PathToken>, path: &str) -> Result<(), String>
  where I: Iterator<Item=(usize, char)> {
  let mut id = String::new();
  id.push(ch);
  let mut next_char = peek(chars);
  while next_char.is_some() {
    let ch = next_char.unwrap();
    if is_identifier_char(ch.1) {
      chars.next();
      id.push(ch.1);
    } else if ch.1 == '.' || ch.1 == '\'' || ch.1 == '[' {
      break;
    } else {
      return Err(format!("\"{}\" is not allowed in an identifier in path expression \"{}\" at index {}",
                         ch.1, path, ch.0));
    }
    next_char = peek(chars);
  }
  tokens.push(PathToken::Field(id));
  Ok(())
}

// path_identifier -> identifier | *
fn path_identifier<I>(chars: &mut Peekable<I>, tokens: &mut Vec<PathToken>, path: &str, index: usize) -> Result<(), String>
  where I: Iterator<Item=(usize, char)> {
  match chars.next() {
    Some(ch) => match ch.1 {
      '*' => {
        tokens.push(PathToken::Star);
        Ok(())
      },
      c if is_identifier_char(c) => {
        identifier(c, chars, tokens, path)?;
        Ok(())
      },
      _ => Err(format!("Expected either a \"*\" or path identifier in path expression \"{}\" at index {}",
                       path, ch.0))
    },
    None => Err(format!("Expected a path after \".\" in path expression \"{}\" at index {}",
                        path, index))
  }
}

// string_path -> [^']+
fn string_path<I>(chars: &mut Peekable<I>, tokens: &mut Vec<PathToken>, path: &str, index: usize) -> Result<(), String>
  where I: Iterator<Item=(usize, char)> {
  let mut id = String::new();
  let mut next_char = peek(chars);
  if next_char.is_some() {
    chars.next();
    let mut ch = next_char.unwrap();
    next_char = peek(chars);
    while ch.1 != '\'' && next_char.is_some() {
      id.push(ch.1);
      chars.next();
      ch = next_char.unwrap();
      next_char = peek(chars);
    }
    if ch.1 == '\'' {
      if id.is_empty() {
        Err(format!("Empty strings are not allowed in path expression \"{}\" at index {}", path, ch.0))
      } else {
        tokens.push(PathToken::Field(id));
        Ok(())
      }
    } else {
      Err(format!("Unterminated string in path expression \"{}\" at index {}", path, ch.0))
    }
  } else {
    Err(format!("Unterminated string in path expression \"{}\" at index {}", path, index))
  }
}

// index_path -> [0-9]+
fn index_path<I>(chars: &mut Peekable<I>, tokens: &mut Vec<PathToken>, path: &str) -> Result<(), String>
  where I: Iterator<Item=(usize, char)> {
  let mut id = String::new();
  let mut next_char = chars.next();
  id.push(next_char.unwrap().1);
  next_char = peek(chars);
  while next_char.is_some() {
    let ch = next_char.unwrap();
    if ch.1.is_numeric() {
      id.push(ch.1);
      chars.next();
    } else {
      break;
    }
    next_char = peek(chars);
  }

  if let Some(ch) = next_char {
    if ch.1 != ']' {
      return Err(format!("Indexes can only consist of numbers or a \"*\", found \"{}\" instead in path expression \"{}\" at index {}",
                         ch.1, path, ch.0))
    }
  }

  tokens.push(PathToken::Index(id.parse().unwrap()));
  Ok(())
}

// bracket_path -> (string_path | index | *) ]
fn bracket_path<I>(chars: &mut Peekable<I>, tokens: &mut Vec<PathToken>, path: &str, index: usize) -> Result<(), String>
  where I: Iterator<Item=(usize, char)> {
  let mut ch = peek(chars);
  match ch {
    Some(c) => {
      if c.1 == '\'' {
        chars.next();
        string_path(chars, tokens, path, c.0)?
      } else if c.1.is_numeric() {
        index_path(chars, tokens, path)?
      } else if c.1 == '*' {
        chars.next();
        tokens.push(PathToken::StarIndex);
      } else if c.1 == ']' {
        return Err(format!("Empty bracket expressions are not allowed in path expression \"{}\" at index {}",
                           path, c.0));
      } else {
        return Err(format!("Indexes can only consist of numbers or a \"*\", found \"{}\" instead in path expression \"{}\" at index {}",
                           c.1, path, c.0));
      };
      ch = peek(chars);
      match ch {
        Some(c) => if c.1 != ']' {
          Err(format!("Unterminated brackets, found \"{}\" instead of \"]\" in path expression \"{}\" at index {}",
                      c.1, path, c.0))
        } else {
          chars.next();
          Ok(())
        },
        None => Err(format!("Unterminated brackets in path expression \"{}\" at index {}",
                            path, path.len() - 1))
      }
    },
    None => Err(format!("Expected a \"'\" (single qoute) or a digit in path expression \"{}\" after index {}",
                        path, index))
  }
}

// path_exp -> (dot-path | bracket-path)*
fn path_exp<I>(chars: &mut Peekable<I>, tokens: &mut Vec<PathToken>, path: &str) -> Result<(), String>
  where I: Iterator<Item=(usize, char)> {
  let mut next_char = chars.next();
  while next_char.is_some() {
    let ch = next_char.unwrap();
    match ch.1 {
      '.' => path_identifier(chars, tokens, path, ch.0)?,
      '[' => bracket_path(chars, tokens, path, ch.0)?,
      _ => return Err(format!("Expected a \".\" or \"[\" instead of \"{}\" in path expression \"{}\" at index {}",
                              ch.1, path, ch.0))
    }
    next_char = chars.next();
  }
  Ok(())
}

fn parse_path_exp(path: &str) -> Result<Vec<PathToken>, String> {
  let mut tokens = vec![];

  // parse_path_exp -> $ path_exp | empty
  let mut chars = path.chars().enumerate().peekable();
  match chars.next() {
    Some(ch) => {
      match ch.1 {
        '$' => {
          tokens.push(PathToken::Root);
          path_exp(&mut chars, &mut tokens, path)?;
          Ok(tokens)
        }
        c if c.is_alphabetic() || c.is_numeric() => {
          tokens.push(PathToken::Root);
          identifier(c, &mut chars, &mut tokens, path)?;
          path_exp(&mut chars, &mut tokens, path)?;
          Ok(tokens)
        }
        _ => Err(format!("Path expression \"{}\" does not start with a root marker \"$\"", path))
      }
    }
    None => Ok(tokens)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use expectest::prelude::*;
  use expectest::expect;

  #[test]
  fn parse_path_exp_handles_empty_string() {
    expect!(parse_path_exp("")).to(be_ok().value(vec![]));
  }

  #[test]
  fn parse_path_exp_handles_root() {
    expect!(parse_path_exp("$")).to(be_ok().value(vec![PathToken::Root]));
  }

  #[test]
  fn parse_path_exp_handles_missing_root() {
    expect!(parse_path_exp("adsjhaskjdh"))
      .to(be_ok().value(vec![PathToken::Root, PathToken::Field(s!("adsjhaskjdh"))]));
  }

  #[test]
  fn parse_path_exp_handles_missing_path() {
    expect!(parse_path_exp("$adsjhaskjdh")).to(
      be_err().value(s!("Expected a \".\" or \"[\" instead of \"a\" in path expression \"$adsjhaskjdh\" at index 1")));
  }

  #[test]
  fn parse_path_exp_handles_missing_path_name() {
    expect!(parse_path_exp("$.")).to(
      be_err().value(s!("Expected a path after \".\" in path expression \"$.\" at index 1")));
    expect!(parse_path_exp("$.a.b.c.")).to(
      be_err().value(s!("Expected a path after \".\" in path expression \"$.a.b.c.\" at index 7")));
  }

  #[test]
  fn parse_path_exp_handles_invalid_identifiers() {
    expect!(parse_path_exp("$.abc!")).to(
      be_err().value(s!("\"!\" is not allowed in an identifier in path expression \"$.abc!\" at index 5")));
    expect!(parse_path_exp("$.a.b.c.}")).to(
      be_err().value(s!("Expected either a \"*\" or path identifier in path expression \"$.a.b.c.}\" at index 8")));
  }

  #[test]
  fn parse_path_exp_with_simple_identifiers() {
    expect!(parse_path_exp("$.a")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("a"))]));
    expect!(parse_path_exp("$.a.b.c")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("a")), PathToken::Field(s!("b")),
                         PathToken::Field(s!("c"))]));
    expect!(parse_path_exp("a.b.c")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("a")), PathToken::Field(s!("b")),
                         PathToken::Field(s!("c"))]));
  }

  #[test]
  fn parse_path_exp_handles_underscores_and_dashes() {
    expect!(parse_path_exp("$.user_id.user-id")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("user_id")),
                         PathToken::Field(s!("user-id"))])
    );
    expect!(parse_path_exp("$._id")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("_id"))])
    );
    expect!(parse_path_exp("$.id:test")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("id:test"))])
    );
  }

  #[test]
  fn parse_path_exp_handles_xml_names() {
    expect!(parse_path_exp("$.foo.@val")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("foo")),
                         PathToken::Field(s!("@val"))])
    );
    expect!(parse_path_exp("$.foo.#text")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("foo")),
                         PathToken::Field(s!("#text"))])
    );
    expect!(parse_path_exp("$.urn:ns:foo.urn:ns:something.#text")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("urn:ns:foo")),
                         PathToken::Field(s!("urn:ns:something")), PathToken::Field(s!("#text"))])
    );
  }

  #[test]
  fn parse_path_exp_with_star_instead_of_identifiers() {
    expect!(parse_path_exp("$.*")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Star]));
    expect!(parse_path_exp("$.a.*.c")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("a")), PathToken::Star,
                         PathToken::Field(s!("c"))]));
  }

  #[test]
  fn parse_path_exp_with_bracket_notation() {
    expect!(parse_path_exp("$['val1']")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("val1"))]));
    expect!(parse_path_exp("$.a['val@1.'].c")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("a")), PathToken::Field(s!("val@1.")),
                         PathToken::Field(s!("c"))]));
    expect!(parse_path_exp("$.a[1].c")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("a")), PathToken::Index(1),
                         PathToken::Field(s!("c"))]));
    expect!(parse_path_exp("$.a[*].c")).to(
      be_ok().value(vec![PathToken::Root, PathToken::Field(s!("a")), PathToken::StarIndex,
                         PathToken::Field(s!("c"))]));
  }

  #[test]
  fn parse_path_exp_with_invalid_bracket_notation() {
    expect!(parse_path_exp("$[")).to(
      be_err().value(s!("Expected a \"'\" (single qoute) or a digit in path expression \"$[\" after index 1")));
    expect!(parse_path_exp("$['")).to(
      be_err().value(s!("Unterminated string in path expression \"$['\" at index 2")));
    expect!(parse_path_exp("$['Unterminated string")).to(
      be_err().value(s!("Unterminated string in path expression \"$['Unterminated string\" at index 21")));
    expect!(parse_path_exp("$['']")).to(
      be_err().value(s!("Empty strings are not allowed in path expression \"$['']\" at index 3")));
    expect!(parse_path_exp("$['test'.b.c")).to(
      be_err().value(s!("Unterminated brackets, found \".\" instead of \"]\" in path expression \"$['test'.b.c\" at index 8")));
    expect!(parse_path_exp("$['test'")).to(
      be_err().value(s!("Unterminated brackets in path expression \"$['test'\" at index 7")));
    expect!(parse_path_exp("$['test']b.c")).to(
      be_err().value(s!("Expected a \".\" or \"[\" instead of \"b\" in path expression \"$[\'test\']b.c\" at index 9")));
  }

  #[test]
  fn parse_path_exp_with_invalid_bracket_index_notation() {
    expect!(parse_path_exp("$[dhghh]")).to(
      be_err().value(s!("Indexes can only consist of numbers or a \"*\", found \"d\" instead in path expression \"$[dhghh]\" at index 2")));
    expect!(parse_path_exp("$[12abc]")).to(
      be_err().value(s!("Indexes can only consist of numbers or a \"*\", found \"a\" instead in path expression \"$[12abc]\" at index 4")));
    expect!(parse_path_exp("$[]")).to(
      be_err().value(s!("Empty bracket expressions are not allowed in path expression \"$[]\" at index 2")));
    expect!(parse_path_exp("$[-1]")).to(
      be_err().value(s!("Indexes can only consist of numbers or a \"*\", found \"-\" instead in path expression \"$[-1]\" at index 2")));
  }

  #[test]
  fn matches_token_test_with_root() {
    expect!(matches_token("$", &PathToken::Root)).to(be_equal_to(2));
    expect!(matches_token("path", &PathToken::Root)).to(be_equal_to(0));
    expect!(matches_token("*", &PathToken::Root)).to(be_equal_to(0));
  }

  #[test]
  fn matches_token_test_with_field() {
    expect!(matches_token("$", &PathToken::Field(s!("path")))).to(be_equal_to(0));
    expect!(matches_token("path", &PathToken::Field(s!("path")))).to(be_equal_to(2));
  }

  #[test]
  fn matches_token_test_with_index() {
    expect!(matches_token("$", &PathToken::Index(2))).to(be_equal_to(0));
    expect!(matches_token("path", &PathToken::Index(2))).to(be_equal_to(0));
    expect!(matches_token("*", &PathToken::Index(2))).to(be_equal_to(0));
    expect!(matches_token("1", &PathToken::Index(2))).to(be_equal_to(0));
    expect!(matches_token("2", &PathToken::Index(2))).to(be_equal_to(2));
  }

  #[test]
  fn matches_token_test_with_index_wildcard() {
    expect!(matches_token("$", &PathToken::StarIndex)).to(be_equal_to(0));
    expect!(matches_token("path", &PathToken::StarIndex)).to(be_equal_to(0));
    expect!(matches_token("*", &PathToken::StarIndex)).to(be_equal_to(0));
    expect!(matches_token("1", &PathToken::StarIndex)).to(be_equal_to(1));
  }

  #[test]
  fn matches_token_test_with_wildcard() {
    expect!(matches_token("$", &PathToken::Star)).to(be_equal_to(1));
    expect!(matches_token("path", &PathToken::Star)).to(be_equal_to(1));
    expect!(matches_token("*", &PathToken::Star)).to(be_equal_to(1));
    expect!(matches_token("1", &PathToken::Star)).to(be_equal_to(1));
  }

  #[test]
  fn matches_path_matches_root_path_element() {
    expect!(calc_path_weight("$", &vec!["$"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$", &vec![]).0 > 0).to(be_false());
  }

  #[test]
  fn matches_path_matches_field_name() {
    expect!(calc_path_weight("$.name", &vec!["$", "name"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$['name']", &vec!["$", "name"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.name.other", &vec!["$", "name", "other"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$['name'].other", &vec!["$", "name", "other"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.name", &vec!["$", "other"]).0 > 0).to(be_false());
    expect!(calc_path_weight("$.name", &vec!["$", "name", "other"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.other", &vec!["$", "name", "other"]).0 > 0).to(be_false());
    expect!(calc_path_weight("$.name.other", &vec!["$", "name"]).0 > 0).to(be_false());
  }

  #[test]
  fn matches_path_matches_array_indices() {
    expect!(calc_path_weight("$[0]", &vec!["$", "0"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.name[1]", &vec!["$", "name", "1"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.name", &vec!["$", "0"]).0 > 0).to(be_false());
    expect!(calc_path_weight("$.name[1]", &vec!["$", "name", "0"]).0 > 0).to(be_false());
    expect!(calc_path_weight("$[1].name", &vec!["$", "name", "1"]).0 > 0).to(be_false());
  }

  #[test]
  fn matches_path_matches_with_wildcard() {
    expect!(calc_path_weight("$[*]", &vec!["$", "0"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.*", &vec!["$", "name"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.*.name", &vec!["$", "some", "name"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.name[*]", &vec!["$", "name", "0"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$.name[*].name", &vec!["$", "name", "1", "name"]).0 > 0).to(be_true());
    expect!(calc_path_weight("$[*]", &vec!["$", "name"]).0 > 0).to(be_false());
  }
}
