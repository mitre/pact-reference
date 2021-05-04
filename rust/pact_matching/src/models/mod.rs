//! The `models` module provides all the structures required to model a Pact.

use std::{fmt, fs};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::default::Default;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::Path;
use std::str;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context};
use base64::{decode, encode};
use fs2::FileExt;
use hex::FromHex;
use itertools::{iproduct, Itertools};
use itertools::EitherOrBoth::{Both, Left, Right};
use lazy_static::*;
use log::*;
use maplit::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use pact_models::{Consumer, PactSpecification, Provider};
use pact_models::content_types::*;
use pact_models::bodies::OptionalBody;

use crate::models::file_utils::{with_read_lock, with_read_lock_for_open_file, with_write_lock};
use crate::models::generators::{Generator, GeneratorCategory};
use crate::models::http_utils::HttpAuth;
use crate::models::json_utils::json_to_string;
use crate::models::matchingrules::MatchingRules;
use crate::models::message::Message;
use crate::models::message_pact::MessagePact;
use crate::models::provider_states::ProviderState;
use crate::models::v4::{AsynchronousMessage, interaction_from_json, SynchronousHttp, V4Interaction, V4Pact};
use crate::models::v4::http_parts::{HttpRequest, HttpResponse};
use crate::models::v4::sync_message::SynchronousMessages;
use crate::path_exp::JSONPath;

pub mod json_utils;
pub mod xml_utils;
#[macro_use] pub mod matchingrules;
#[macro_use] pub mod generators;
pub mod http_utils;
mod expression_parser;
mod file_utils;

/// Version of the library
pub const PACT_RUST_VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

/// Enumeration of the types of differences between requests and responses
#[derive(PartialEq, Debug, Clone, Eq)]
pub enum DifferenceType {
  /// Methods differ
  Method,
  /// Paths differ
  Path,
  /// Headers differ
  Headers,
  /// Query parameters differ
  QueryParameters,
  /// Bodies differ
  Body,
  /// Matching Rules differ
  MatchingRules,
  /// Response status differ
  Status
}

/// Trait to specify an HTTP part of a message. It encapsulates the shared parts of a request and
/// response.
pub trait HttpPart {
    /// Returns the headers of the HTTP part.
    fn headers(&self) -> &Option<HashMap<String, Vec<String>>>;

    /// Returns the headers of the HTTP part in a mutable form.
    fn headers_mut(&mut self) -> &mut HashMap<String, Vec<String>>;

    /// Returns the body of the HTTP part.
    fn body(&self) -> &OptionalBody;

    /// Returns the matching rules of the HTTP part.
    fn matching_rules(&self) -> &matchingrules::MatchingRules;

    /// Returns the generators of the HTTP part.
    fn generators(&self) -> &generators::Generators;

    /// Lookup up the content type for the part
    fn lookup_content_type(&self) -> Option<String>;

    /// Tries to detect the content type of the body by matching some regular expressions against
    /// the first 32 characters.
    fn detect_content_type(&self) -> Option<ContentType> {
      match *self.body() {
        OptionalBody::Present(ref body, _) => {
          let s: String = match str::from_utf8(body) {
            Ok(s) => s.to_string(),
            Err(_) => String::new()
          };
          detect_content_type_from_string(&s)
        },
        _ => None
      }
    }

  /// Determine the content type of the HTTP part. If a `Content-Type` header is present, the
  /// value of that header will be returned. Otherwise, the body will be inspected.
  fn content_type(&self) -> Option<ContentType> {
    let body = self.body();
    if body.has_content_type() {
      body.content_type()
    } else {
      match self.lookup_content_type() {
        Some(ref h) => match ContentType::parse(h.as_str()) {
          Ok(v) => Some(v),
          Err(_) => self.detect_content_type()
        },
        None => self.detect_content_type()
      }
    }
  }

  /// Checks if the HTTP Part has the given header
  fn has_header(&self, header_name: &str) -> bool {
      self.lookup_header_value(header_name).is_some()
  }

  /// Checks if the HTTP Part has the given header
  fn lookup_header_value(&self, header_name: &str) -> Option<String> {
    match *self.headers() {
      Some(ref h) => h.iter()
        .find(|kv| kv.0.to_lowercase() == header_name.to_lowercase())
        .map(|kv| kv.1.clone().join(", ")),
      None => None
    }
  }

  /// If the body is a textual type (non-binary)
  fn has_text_body(&self) -> bool {
    let body = self.body();
    let str_body = body.str_value();
    body.is_present() && !str_body.is_empty() && str_body.is_ascii()
  }

  /// Convenience method to add a header
  fn add_header(&mut self, key: &str, val: Vec<&str>) {
    let headers = self.headers_mut();
    headers.insert(key.to_string(), val.iter().map(|v| v.to_string()).collect());
  }

  /// Builds a map of generators from the generators and matching rules
  fn build_generators(&self, category: &GeneratorCategory) -> HashMap<JSONPath, Generator> {
    let mut generators = hashmap!{};
    if let Some(generators_for_category) = self.generators().categories.get(category) {
      for (path, generator) in generators_for_category {
        generators.insert(path.clone(), generator.clone());
      }
    }
    if let Some(rules) = self.matching_rules().rules_for_category(category.clone().into()) {
      for (path, generator) in rules.generators() {
        generators.insert(path.clone(), generator.clone());
      }
    }
    generators
  }
}

/// Struct that defines the request.
#[derive(Serialize, Deserialize, Debug, Clone, Eq)]
pub struct Request {
    /// Request method
    pub method: String,
    /// Request path
    pub path: String,
    /// Request query string
    pub query: Option<HashMap<String, Vec<String>>>,
    /// Request headers
    pub headers: Option<HashMap<String, Vec<String>>>,
    /// Request body
    pub body: OptionalBody,
    /// Request matching rules
    pub matching_rules: matchingrules::MatchingRules,
    /// Request generators
    pub generators: generators::Generators
}

impl HttpPart for Request {
  fn headers(&self) -> &Option<HashMap<String, Vec<String>>> {
        &self.headers
    }

  fn headers_mut(&mut self) -> &mut HashMap<String, Vec<String>> {
    if self.headers.is_none() {
      self.headers = Some(hashmap!{});
    }
    self.headers.as_mut().unwrap()
  }

  fn body(&self) -> &OptionalBody {
      &self.body
  }

  fn matching_rules(&self) -> &matchingrules::MatchingRules {
      &self.matching_rules
  }

  fn generators(&self) -> &generators::Generators {
      &self.generators
    }

  fn lookup_content_type(&self) -> Option<String> {
    self.lookup_header_value(&"content-type".to_string())
  }
}

impl Hash for Request {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.path.hash(state);
        if self.query.is_some() {
            for (k, v) in self.query.clone().unwrap() {
                k.hash(state);
                v.hash(state);
            }
        }
        if self.headers.is_some() {
            for (k, v) in self.headers.clone().unwrap() {
                k.hash(state);
                v.hash(state);
            }
        }
        self.body.hash(state);
        self.matching_rules.hash(state);
        self.generators.hash(state);
    }
}

impl PartialEq for Request {
  fn eq(&self, other: &Self) -> bool {
    self.method == other.method && self.path == other.path && self.query == other.query &&
      self.headers == other.headers && self.body == other.body &&
      self.matching_rules == other.matching_rules && self.generators == other.generators
  }

  fn ne(&self, other: &Self) -> bool {
    self.method != other.method || self.path != other.path || self.query != other.query ||
      self.headers != other.headers || self.body != other.body ||
      self.matching_rules != other.matching_rules || self.generators != other.generators
  }
}

impl Display for Request {
  fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
    write!(f, "Request ( method: {}, path: {}, query: {:?}, headers: {:?}, body: {} )",
      self.method, self.path, self.query, self.headers, self.body)
  }
}

impl Default for Request {
  fn default() -> Self {
    Request {
      method: s!("GET"),
      path: s!("/"),
      query: None,
      headers: None,
      body: OptionalBody::Missing,
      matching_rules: matchingrules::MatchingRules::default(),
      generators: generators::Generators::default()
    }
  }
}

fn headers_from_json(request: &Value) -> Option<HashMap<String, Vec<String>>> {
  match request.get("headers") {
    Some(v) => match *v {
      Value::Object(ref m) => Some(m.iter().map(|(key, val)| {
        match val {
          &Value::String(ref s) => (key.clone(), s.clone().split(',').map(|v| s!(v.trim())).collect()),
          &Value::Array(ref v) => (key.clone(), v.iter().map(|val| {
            match val {
              &Value::String(ref s) => s.clone(),
              _ => val.to_string()
            }
          }).collect()),
          _ => (key.clone(), vec![val.to_string()])
        }
      }).collect()),
      _ => None
    },
    None => None
  }
}

fn headers_to_json(headers: &HashMap<String, Vec<String>>) -> Value {
  json!(headers.iter().fold(BTreeMap::new(), |mut map, kv| {
    map.insert(kv.0.clone(), Value::String(kv.1.join(", ")));
    map
  }))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum JsonParsable {
    JsonStringValue(String),
    KeyValue(HashMap<String, Value>)
}

fn body_from_json(request: &Value, fieldname: &str, headers: &Option<HashMap<String, Vec<String>>>) -> OptionalBody {
  let content_type = match headers {
    &Some(ref h) => match h.iter().find(|kv| kv.0.to_lowercase() == s!("content-type")) {
      Some(kv) => {
        match ContentType::parse(kv.1[0].as_str()) {
          Ok(v) => Some(v),
          Err(_) => None
        }
      },
      None => None
    },
    &None => None
  };

  match request.get(fieldname) {
    Some(v) => match v {
      Value::String(s) => {
        if s.is_empty() {
          OptionalBody::Empty
        } else {
          let content_type = content_type.unwrap_or_else(|| {
            detect_content_type_from_string(s).unwrap_or_default()
          });
          if content_type.is_json() {
            match serde_json::from_str::<JsonParsable>(&s) {
              Ok(_) => OptionalBody::Present(s.clone().into(), Some(content_type)),
              Err(_) => OptionalBody::Present(format!("\"{}\"", s).into(), Some(content_type))
            }
          } else if content_type.is_text() {
            OptionalBody::Present(s.clone().into(), Some(content_type))
          } else {
            match decode(s) {
              Ok(bytes) => OptionalBody::Present(bytes.into(), None),
              Err(_) => OptionalBody::Present(s.clone().into(), None)
            }
          }
        }
      },
      Value::Null => OptionalBody::Null,
      _ => OptionalBody::Present(v.to_string().into(), None)
    },
    None => OptionalBody::Missing
  }
}

/// Converts a query string map into a query string
pub fn build_query_string(query: HashMap<String, Vec<String>>) -> String {
    query.into_iter()
        .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
        .flat_map(|kv| {
            kv.1.iter()
                .map(|v| format!("{}={}", kv.0, encode_query(v)))
                .collect_vec()
        })
        .join("&")
}

fn query_from_json(query_json: &Value, spec_version: &PactSpecification) -> Option<HashMap<String, Vec<String>>> {
    match query_json {
        &Value::String(ref s) => parse_query_string(s),
        _ => {
            log::warn!("Only string versions of request query strings are supported with specification version {}, ignoring.",
                spec_version.to_string());
            None
        }
    }
}

fn v3_query_from_json(query_json: &Value, spec_version: &PactSpecification) -> Option<HashMap<String, Vec<String>>> {
    match query_json {
        &Value::String(ref s) => parse_query_string(s),
        &Value::Object(ref map) => Some(map.iter().map(|(k, v)| {
            (k.clone(), match v {
                &Value::String(ref s) => vec![s.clone()],
                &Value::Array(ref array) => array.iter().map(|item| match item {
                    &Value::String(ref s) => s.clone(),
                    _ => v.to_string()
                }).collect(),
                _ => {
                    log::warn!("Query paramter value '{}' is not valid, ignoring", v);
                    vec![]
                }
            })
        }).collect()),
        _ => {
            log::warn!("Only string or map versions of request query strings are supported with specification version {}, ignoring.",
                spec_version.to_string());
            None
        }
    }
}

fn query_to_json(query: HashMap<String, Vec<String>>, spec_version: &PactSpecification) -> Value {
  match spec_version {
    &PactSpecification::V3 | &PactSpecification::V4 => Value::Object(query.iter().map(|(k, v)| {
      (k.clone(), Value::Array(v.iter().map(|q| Value::String(q.clone())).collect()))}
    ).collect()),
    _ => Value::String(build_query_string(query))
  }
}

impl Request {
    /// Builds a `Request` from a `Value` struct.
    pub fn from_json(request_json: &Value, spec_version: &PactSpecification) -> anyhow::Result<Request> {
        let method_val = match request_json.get("method") {
            Some(v) => match *v {
                Value::String(ref s) => s.to_uppercase(),
                _ => v.to_string().to_uppercase()
            },
            None => "GET".to_string()
        };
        let path_val = match request_json.get("path") {
            Some(v) => match *v {
                Value::String(ref s) => s.clone(),
                _ => v.to_string()
            },
            None => "/".to_string()
        };
        let query_val = match request_json.get("query") {
            Some(v) => match spec_version {
                &PactSpecification::V3 => v3_query_from_json(v, spec_version),
                _ => query_from_json(v, spec_version)
            },
            None => None
        };
        let headers = headers_from_json(request_json);
        Ok(Request {
            method: method_val,
            path: path_val,
            query: query_val,
            headers: headers.clone(),
            body: body_from_json(request_json, "body", &headers),
            matching_rules: matchingrules::matchers_from_json(request_json, &Some(s!("requestMatchingRules")))?,
            generators: generators::generators_from_json(request_json)?
        })
    }

    /// Converts this `Request` to a `Value` struct.
    pub fn to_json(&self, spec_version: &PactSpecification) -> Value {
        let mut json = json!({
            s!("method") : Value::String(self.method.to_uppercase()),
            s!("path") : Value::String(self.path.clone())
        });
        {
            let map = json.as_object_mut().unwrap();
            if self.query.is_some() {
                map.insert(s!("query"), query_to_json(self.query.clone().unwrap(), spec_version));
            }
            if self.headers.is_some() {
                map.insert(s!("headers"), headers_to_json(&self.headers.clone().unwrap()));
            }
            match self.body {
              OptionalBody::Present(ref body, _) => if self.content_type().unwrap_or_default().is_json() {
                match serde_json::from_slice(body) {
                  Ok(json_body) => { map.insert(s!("body"), json_body); },
                  Err(err) => {
                    log::warn!("Failed to parse json body: {}", err);
                    map.insert(s!("body"), Value::String(encode(body)));
                  }
                }
              } else {
                match str::from_utf8(body) {
                  Ok(s) => map.insert(s!("body"), Value::String(s.to_string())),
                  Err(_) => map.insert(s!("body"), Value::String(encode(body)))
                };
              },
              OptionalBody::Empty => { map.insert(s!("body"), Value::String(s!(""))); },
              OptionalBody::Missing => (),
              OptionalBody::Null => { map.insert(s!("body"), Value::Null); }
            }
            if self.matching_rules.is_not_empty() {
                map.insert(s!("matchingRules"), matchingrules::matchers_to_json(
                &self.matching_rules.clone(), spec_version));
            }
            if self.generators.is_not_empty() {
              map.insert(s!("generators"), generators::generators_to_json(
                &self.generators.clone(), spec_version));
            }
        }
        json
    }

    /// Returns the default request: a GET request to the root.
    #[deprecated(since="0.6.0", note="please use `default()` from the standard Default trait instead")]
    pub fn default_request() -> Request {
      Request::default()
    }

    /// Return a description of all the differences from the other request
    pub fn differences_from(&self, other: &Request) -> Vec<(DifferenceType, String)> {
        let mut differences = vec![];
        if self.method != other.method {
            differences.push((DifferenceType::Method, format!("Request method {} != {}", self.method, other.method)));
        }
        if self.path != other.path {
            differences.push((DifferenceType::Path, format!("Request path {} != {}", self.path, other.path)));
        }
        if self.query != other.query {
            differences.push((DifferenceType::QueryParameters, format!("Request query {:?} != {:?}", self.query, other.query)));
        }
        let mut keys = self.headers.clone().map(|m| m.keys().cloned().collect_vec()).unwrap_or_default();
        let mut other_keys = other.headers.clone().map(|m| m.keys().cloned().collect_vec()).unwrap_or_default();
        keys.sort();
        other_keys.sort();
        if keys != other_keys {
            differences.push((DifferenceType::Headers, format!("Request headers {:?} != {:?}", self.headers, other.headers)));
        }
        if self.body != other.body {
            differences.push((DifferenceType::Body, format!("Request body '{:?}' != '{:?}'", self.body, other.body)));
        }
        if self.matching_rules != other.matching_rules {
            differences.push((DifferenceType::MatchingRules, format!("Request matching rules {:?} != {:?}", self.matching_rules, other.matching_rules)));
        }
        differences
    }

  /// Convert this interaction to V4 format
  pub fn as_v4_request(&self) -> HttpRequest {
    HttpRequest {
      method: self.method.clone(),
      path: self.path.clone(),
      query: self.query.clone(),
      headers: self.headers.clone(),
      body: self.body.clone(),
      matching_rules: self.matching_rules.clone(),
      generators: self.generators.clone()
    }
  }
}

/// Struct that defines the response.
#[derive(Debug, Clone, Eq)]
pub struct Response {
    /// Response status
    pub status: u16,
    /// Response headers
    pub headers: Option<HashMap<String, Vec<String>>>,
    /// Response body
    pub body: OptionalBody,
    /// Response matching rules
    pub matching_rules: matchingrules::MatchingRules,
    /// Response generators
    pub generators: generators::Generators
}

impl Response {

    /// Build a `Response` from a `Value` struct.
    pub fn from_json(response: &Value, _: &PactSpecification) -> anyhow::Result<Response> {
        let status_val = match response.get("status") {
            Some(v) => v.as_u64().unwrap() as u16,
            None => 200
        };
        let headers = headers_from_json(response);
        Ok(Response {
            status: status_val,
            headers: headers.clone(),
            body: body_from_json(response, "body", &headers),
            matching_rules:  matchingrules::matchers_from_json(response, &Some(s!("responseMatchingRules")))?,
            generators:  generators::generators_from_json(response)?
        })
    }

    /// Returns a default response: Status 200
    #[deprecated(since="0.5.4", note="please use `default()` from the standard Default trait instead")]
    pub fn default_response() -> Response {
      Response::default()
    }

    /// Converts this response to a `Value` struct.
    #[allow(unused_variables)]
    pub fn to_json(&self, spec_version: &PactSpecification) -> Value {
      let mut json = json!({
        "status" : json!(self.status)
      });
      {
        let map = json.as_object_mut().unwrap();
        if self.headers.is_some() {
          map.insert(s!("headers"), headers_to_json(&self.headers.clone().unwrap()));
        }
        match self.body {
          OptionalBody::Present(ref body, _) => {
            if self.content_type().unwrap_or_default().is_json() {
              match serde_json::from_slice(body) {
                Ok(json_body) => { map.insert(s!("body"), json_body); },
                Err(err) => {
                  log::warn!("Failed to parse json body: {}", err);
                  map.insert(s!("body"), Value::String(encode(body)));
                }
              }
            } else {
              match str::from_utf8(body) {
                Ok(s) => map.insert(s!("body"), Value::String(s.to_string())),
                Err(_) => map.insert(s!("body"), Value::String(encode(body)))
              };
            }
          },
          OptionalBody::Empty => { map.insert(s!("body"), Value::String(s!(""))); },
          OptionalBody::Missing => (),
          OptionalBody::Null => { map.insert(s!("body"), Value::Null); }
        }
        if self.matching_rules.is_not_empty() {
          map.insert(s!("matchingRules"), matchingrules::matchers_to_json(
            &self.matching_rules.clone(), spec_version));
        }
        if self.generators.is_not_empty() {
          map.insert(s!("generators"), generators::generators_to_json(
            &self.generators.clone(), spec_version));
        }
      }
      json
    }

    /// Return a description of all the differences from the other response
    pub fn differences_from(&self, other: &Response) -> Vec<(DifferenceType, String)> {
        let mut differences = vec![];
        if self.status != other.status {
            differences.push((DifferenceType::Status, format!("Response status {} != {}", self.status, other.status)));
        }
        if self.headers != other.headers {
            differences.push((DifferenceType::Headers, format!("Response headers {:?} != {:?}", self.headers, other.headers)));
        }
        if self.body != other.body {
            differences.push((DifferenceType::Body, format!("Response body '{:?}' != '{:?}'", self.body, other.body)));
        }
        if self.matching_rules != other.matching_rules {
            differences.push((DifferenceType::MatchingRules, format!("Response matching rules {:?} != {:?}", self.matching_rules, other.matching_rules)));
        }
        differences
    }

  /// Convert this response to V4 format
  pub fn as_v4_response(&self) -> HttpResponse {
    HttpResponse {
      status: self.status,
      headers: self.headers.clone(),
      body: self.body.clone(),
      matching_rules: self.matching_rules.clone(),
      generators: self.generators.clone()
    }
  }
}

impl HttpPart for Response {
  fn headers(&self) -> &Option<HashMap<String, Vec<String>>> {
        &self.headers
    }

  fn headers_mut(&mut self) -> &mut HashMap<String, Vec<String>> {
    if self.headers.is_none() {
      self.headers = Some(hashmap!{});
    }
    self.headers.as_mut().unwrap()
  }

  fn body(&self) -> &OptionalBody {
      &self.body
  }

  fn matching_rules(&self) -> &matchingrules::MatchingRules {
      &self.matching_rules
  }

  fn generators(&self) -> &generators::Generators {
      &self.generators
    }

  fn lookup_content_type(&self) -> Option<String> {
    self.lookup_header_value(&"content-type".to_string())
  }
}

impl Hash for Response {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.status.hash(state);
        if self.headers.is_some() {
            for (k, v) in self.headers.clone().unwrap() {
                k.hash(state);
                v.hash(state);
            }
        }
        self.body.hash(state);
        self.matching_rules.hash(state);
        self.generators.hash(state);
    }
}

impl PartialEq for Response {
  fn eq(&self, other: &Self) -> bool {
    self.status == other.status && self.headers == other.headers && self.body == other.body &&
      self.matching_rules == other.matching_rules && self.generators == other.generators
  }

  fn ne(&self, other: &Self) -> bool {
    self.status != other.status || self.headers != other.headers || self.body != other.body ||
      self.matching_rules != other.matching_rules || self.generators != other.generators
  }
}

impl Display for Response {
  fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
    write!(f, "Response ( status: {}, headers: {:?}, body: {} )", self.status, self.headers,
           self.body)
  }
}

impl Default for Response {
  fn default() -> Self {
    Response {
      status: 200,
      headers: None,
      body: OptionalBody::Missing,
      matching_rules: matchingrules::MatchingRules::default(),
      generators: generators::Generators::default()
    }
  }
}

pub mod provider_states;

/// Struct that defined an interaction conflict
#[derive(Debug, Clone)]
pub struct PactConflict {
    /// Description of the interactions
    pub interaction: String,
    /// Conflict description
    pub description: String
}

/// Interaction Trait
pub trait Interaction: Debug {
  /// The type of the interaction
  fn type_of(&self) -> String;

  /// If this is a request/response interaction
  fn is_request_response(&self) -> bool;

  /// Returns the request/response interaction if it is one
  fn as_request_response(&self) -> Option<RequestResponseInteraction>;

  /// If this is a message interaction
  fn is_message(&self) -> bool;

  /// Returns the message interaction if it is one
  fn as_message(&self) -> Option<Message>;

  /// Interaction ID. This will only be set if the Pact file was fetched from a Pact Broker
  fn id(&self) -> Option<String>;

  /// Description of this interaction. This needs to be unique in the pact file.
  fn description(&self) -> String;

  /// Optional provider states for the interaction.
  /// See https://docs.pact.io/getting_started/provider_states for more info on provider states.
  fn provider_states(&self) -> Vec<provider_states::ProviderState>;

  /// Body of the response or message
  #[deprecated(
    since = "0.8.14",
    note = "Some interactions have multiple contents (like request/response), so it is impossible \
      to know which to return for this method"
  )]
  fn contents(&self) -> OptionalBody;

  /// Determine the content type of the interaction. If a `Content-Type` header or metadata value is present, the
  /// value of that value will be returned. Otherwise, the contents will be inspected.
  #[deprecated(
  since = "0.8.14",
  note = "Some interactions have multiple contents (like request/response), so it is impossible \
      to know which to return for this method"
  )]
  fn content_type(&self) -> Option<ContentType>;

  /// If this is a V4 interaction
  fn is_v4(&self) -> bool;

  /// Returns the interaction in V4 format
  fn as_v4(&self) -> Option<Box<dyn V4Interaction>>;

  /// Returns the interaction in V4 format
  fn as_v4_http(&self) -> Option<SynchronousHttp>;

  /// Returns the interaction in V4 format
  fn as_v4_async_message(&self) -> Option<AsynchronousMessage>;

  /// Returns the interaction in V4 format
  fn as_v4_sync_message(&self) -> Option<SynchronousMessages>;

  /// Clones this interaction and wraps it in a Box
  fn boxed(&self) -> Box<dyn Interaction + Send>;

  /// Clones this interaction and wraps it in an Arc
  fn arced(&self) -> Arc<dyn Interaction + Send>;

  /// Clones this interaction and wraps it in an Arc and Mutex
  fn thread_safe(&self) -> Arc<Mutex<dyn Interaction + Send + Sync>>;

  /// Returns the matching rules associated with this interaction (if there are any)
  fn matching_rules(&self) -> Option<MatchingRules>;
}

impl Display for dyn Interaction {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    if let Some(req_res) = self.as_request_response() {
      std::fmt::Display::fmt(&req_res, f)
    } else if let Some(mp) = self.as_message() {
      std::fmt::Display::fmt(&mp, f)
    } else if let Some(mp) = self.as_v4_http() {
      std::fmt::Display::fmt(&mp, f)
    } else if let Some(mp) = self.as_v4_async_message() {
      std::fmt::Display::fmt(&mp, f)
    } else {
      Err(fmt::Error)
    }
  }
}

impl Clone for Box<dyn Interaction> {
  fn clone(&self) -> Self {
    if self.is_v4() {
      if let Some(http) = self.as_v4_http() {
        Box::new(http)
      } else if let Some(message) = self.as_v4_async_message() {
        Box::new(message)
      } else {
        panic!("Internal Error - Tried to clone an interaction that was not valid")
      }
    } else if let Some(req_res) = self.as_request_response() {
      Box::new(req_res)
    } else if let Some(mp) = self.as_message() {
      Box::new(mp)
    } else {
      panic!("Internal Error - Tried to clone an interaction that was not valid")
    }
  }
}

/// Struct that defines an interaction (request and response pair)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestResponseInteraction {
    /// Interaction ID. This will only be set if the Pact file was fetched from a Pact Broker
    pub id: Option<String>,
    /// Description of this interaction. This needs to be unique in the pact file.
    pub description: String,
    /// Optional provider states for the interaction.
    /// See https://docs.pact.io/getting_started/provider_states for more info on provider states.
    pub provider_states: Vec<provider_states::ProviderState>,
    /// Request of the interaction
    pub request: Request,
    /// Response of the interaction
    pub response: Response
}

impl Interaction for RequestResponseInteraction {
  fn type_of(&self) -> String {
    "V3 Synchronous/HTTP".into()
  }

  fn is_request_response(&self) -> bool {
    true
  }

  fn as_request_response(&self) -> Option<RequestResponseInteraction> {
    Some(self.clone())
  }

  fn is_message(&self) -> bool {
    false
  }

  fn as_message(&self) -> Option<Message> {
    None
  }

  fn id(&self) -> Option<String> {
    self.id.clone()
  }

  fn description(&self) -> String {
    self.description.clone()
  }

  fn provider_states(&self) -> Vec<ProviderState> {
    self.provider_states.clone()
  }

  fn contents(&self) -> OptionalBody {
    self.response.body.clone()
  }

  fn content_type(&self) -> Option<ContentType> {
    self.response.content_type()
  }

  fn is_v4(&self) -> bool {
    false
  }

  fn as_v4(&self) -> Option<Box<dyn V4Interaction>> {
    self.as_v4_http().map(|i| i.boxed_v4())
  }

  fn as_v4_http(&self) -> Option<SynchronousHttp> {
    Some(SynchronousHttp {
      id: self.id.clone(),
      key: None,
      description: self.description.clone(),
      provider_states: self.provider_states.clone(),
      request: self.request.as_v4_request(),
      response: self.response.as_v4_response(),
      .. Default::default()
    }.with_key())
  }

  fn as_v4_async_message(&self) -> Option<AsynchronousMessage> {
    None
  }

  fn as_v4_sync_message(&self) -> Option<SynchronousMessages> {
    None
  }

  fn boxed(&self) -> Box<dyn Interaction + Send> {
    Box::new(self.clone())
  }

  fn arced(&self) -> Arc<dyn Interaction + Send> {
    Arc::new(self.clone())
  }

  fn thread_safe(&self) -> Arc<Mutex<dyn Interaction + Send + Sync>> {
    Arc::new(Mutex::new(self.clone()))
  }

  fn matching_rules(&self) -> Option<MatchingRules> {
    None
  }
}

impl RequestResponseInteraction {
    /// Constructs an `Interaction` from the `Value` struct.
    pub fn from_json(index: usize, pact_json: &Value, spec_version: &PactSpecification)
    -> anyhow::Result<RequestResponseInteraction> {
        let id = pact_json.get("_id").map(|id| json_to_string(id));
        let description = match pact_json.get("description") {
            Some(v) => match *v {
                Value::String(ref s) => s.clone(),
                _ => v.to_string()
            },
            None => format!("Interaction {}", index)
        };
        let provider_states = provider_states::ProviderState::from_json(pact_json);
        let request = match pact_json.get("request") {
            Some(v) => Request::from_json(v, spec_version)?,
            None => Request::default()
        };
        let response = match pact_json.get("response") {
            Some(v) => Response::from_json(v, spec_version)?,
            None => Response::default()
        };
      Ok(RequestResponseInteraction {
          id,
          description,
          provider_states,
          request,
          response
      })
    }

    /// Converts this interaction to a `Value` struct.
    pub fn to_json(&self, spec_version: &PactSpecification) -> Value {
        let mut value = json!({
            s!("description"): Value::String(self.description.clone()),
            s!("request"): self.request.to_json(spec_version),
            s!("response"): self.response.to_json(spec_version)
        });
        if !self.provider_states.is_empty() {
            let map = value.as_object_mut().unwrap();
            match spec_version {
                &PactSpecification::V3 => map.insert(s!("providerStates"),
                                                     Value::Array(self.provider_states.iter().map(|p| p.to_json()).collect())),
                _ => map.insert(s!("providerState"), Value::String(
                    self.provider_states.first().unwrap().name.clone()))
            };
        }
        value
    }

    /// Returns list of conflicts if this interaction conflicts with the other interaction.
    ///
    /// Two interactions conflict if they have the same description and provider state, but they request and
    /// responses are not equal
    pub fn conflicts_with(&self, other: &dyn Interaction) -> Vec<PactConflict> {
      if let Some(other) = other.as_request_response() {
        if self.description == other.description && self.provider_states == other.provider_states {
          let mut conflicts = self.request.differences_from(&other.request).iter()
            .filter(|difference| match difference.0 {
              DifferenceType::MatchingRules | DifferenceType::Body => false,
              _ => true
            })
            .map(|difference| PactConflict { interaction: self.description.clone(), description: difference.1.clone() })
            .collect::<Vec<PactConflict>>();
          for difference in self.response.differences_from(&other.response) {
            match difference.0 {
              DifferenceType::MatchingRules | DifferenceType::Body => (),
              _ => conflicts.push(PactConflict { interaction: self.description.clone(), description: difference.1.clone() })
            };
          }
          conflicts
        } else {
          vec![]
        }
      } else {
        vec![PactConflict {
          interaction: self.description.clone(),
          description: format!("You can not combine message and request/response interactions")
        }]
      }
    }
}

impl Default for RequestResponseInteraction {
  fn default() -> Self {
    RequestResponseInteraction {
      id: None,
      description: s!("Default Interaction"),
      provider_states: vec![],
      request: Request::default(),
      response: Response::default()
    }
  }
}

impl Display for RequestResponseInteraction {
  fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
    write!(f, "Interaction ( id: {:?}, description: \"{}\", provider_states: {:?}, request: {}, response: {} )",
           self.id, self.description, self.provider_states, self.request, self.response)
  }
}

/// Trait for a Pact (request/response or message)
pub trait Pact: Debug + ReadWritePact {
  /// Consumer side of the pact
  fn consumer(&self) -> Consumer;
  /// Provider side of the pact
  fn provider(&self) -> Provider;
  /// Interactions in the Pact
  fn interactions(&self) -> Vec<&dyn Interaction>;
  /// Pact metadata
  fn metadata(&self) -> BTreeMap<String, BTreeMap<String, String>>;
  /// Converts this pact to a `Value` struct.
  fn to_json(&self, pact_spec: PactSpecification) -> anyhow::Result<Value>;
  /// Attempt to downcast to a concrete Pact
  fn as_request_response_pact(&self) -> anyhow::Result<RequestResponsePact>;
  /// Attempt to downcast to a concrete Message Pact
  fn as_message_pact(&self) -> anyhow::Result<MessagePact>;
  /// Attempt to downcast to a concrete V4 Pact
  fn as_v4_pact(&self) -> anyhow::Result<V4Pact>;
  /// Specification version of this Pact
  fn specification_version(&self) -> PactSpecification;
  /// Clones this Pact and wraps it in a Box
  fn boxed(&self) -> Box<dyn Pact + Send>;
  /// Clones this Pact and wraps it in an Arc
  fn arced(&self) -> Arc<dyn Pact + Send>;
  /// Clones this Pact and wraps it in an Arc and Mutex
  fn thread_safe(&self) -> Arc<Mutex<dyn Pact + Send + Sync>>;
  /// Adds an interactions in the Pact
  fn add_interaction(&mut self, interaction: &dyn Interaction) -> anyhow::Result<()>;
}

pub mod message;
pub mod message_pact;
pub mod v4;

/// Struct that represents a pact between the consumer and provider of a service.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequestResponsePact {
    /// Consumer side of the pact
    pub consumer: Consumer,
    /// Provider side of the pact
    pub provider: Provider,
    /// List of interactions between the consumer and provider.
    pub interactions: Vec<RequestResponseInteraction>,
    /// Metadata associated with this pact file.
    pub metadata: BTreeMap<String, BTreeMap<String, String>>,
    /// Specification version of this pact
    pub specification_version: PactSpecification
}

impl Pact for RequestResponsePact {
  fn consumer(&self) -> Consumer {
    self.consumer.clone()
  }

  fn provider(&self) -> Provider {
    self.provider.clone()
  }

  fn interactions(&self) -> Vec<&dyn Interaction> {
    self.interactions.iter().map(|i| i as &dyn Interaction).collect()
  }

  fn metadata(&self) -> BTreeMap<String, BTreeMap<String, String>> {
    self.metadata.clone()
  }

  /// Converts this pact to a `Value` struct.
  fn to_json(&self, pact_spec: PactSpecification) -> anyhow::Result<Value> {
    match pact_spec {
      PactSpecification::V4 => self.as_v4_pact()?.to_json(pact_spec),
      _ => Ok(json!({
          "consumer": self.consumer.to_json(),
          "provider": self.provider.to_json(),
          "interactions": Value::Array(self.interactions.iter().map(|i| i.to_json(&pact_spec)).collect()),
          "metadata": self.metadata_to_json(&pact_spec)
      }))
    }
  }

  fn as_request_response_pact(&self) -> anyhow::Result<RequestResponsePact> {
    Ok(self.clone())
  }

  fn as_message_pact(&self) -> anyhow::Result<MessagePact> {
    Err(anyhow!("Can't convert a Request/response Pact to a different type"))
  }

  fn as_v4_pact(&self) -> anyhow::Result<V4Pact> {
    let interactions = self.interactions.iter()
      .map(|i| i.as_v4())
      .filter(|i| i.is_some())
      .map(|i| i.unwrap())
      .collect();
    Ok(V4Pact {
      consumer: self.consumer.clone(),
      provider: self.provider.clone(),
      interactions,
      metadata: self.metadata.iter().map(|(k, v)| (k.clone(), json!(v))).collect()
    })
  }

  fn specification_version(&self) -> PactSpecification {
    self.specification_version.clone()
  }

  fn boxed(&self) -> Box<dyn Pact + Send> {
    Box::new(self.clone())
  }

  fn arced(&self) -> Arc<dyn Pact + Send> {
    Arc::new(self.clone())
  }

  fn thread_safe(&self) -> Arc<Mutex<dyn Pact + Send + Sync>> {
    Arc::new(Mutex::new(self.clone()))
  }

  fn add_interaction(&mut self, interaction: &dyn Interaction) -> anyhow::Result<()> {
    match interaction.as_request_response() {
      None => Err(anyhow!("Can only add request/response interactions to this Pact")),
      Some(interaction) => {
        self.interactions.push(interaction);
        Ok(())
      }
    }
  }
}

fn parse_meta_data(pact_json: &Value) -> BTreeMap<String, BTreeMap<String, String>> {
    match pact_json.get("metadata") {
        Some(v) => match *v {
            Value::Object(ref obj) => obj.iter().map(|(k, v)| {
                let val = match *v {
                    Value::Object(ref obj) => obj.iter().map(|(k, v)| {
                        match *v {
                            Value::String(ref s) => (k.clone(), s.clone()),
                            _ => (k.clone(), v.to_string())
                        }
                    }).collect(),
                    _ => btreemap!{}
                };
                let key = match k.as_str() {
                  "pact-specification" => s!("pactSpecification"),
                  "pact-rust" => s!("pactRust"),
                  _ => k.clone()
                };
                (key, val)
            }).collect(),
            _ => btreemap!{}
        },
        None => btreemap!{}
    }
}

fn parse_interactions(pact_json: &Value, spec_version: PactSpecification) -> anyhow::Result<Vec<RequestResponseInteraction>> {
    match pact_json.get("interactions") {
        Some(v) => match *v {
            Value::Array(ref array) => {
              let mut interactions = Vec::new();
              for (index, ijson) in array.iter().enumerate() {
                interactions.push(
                  RequestResponseInteraction::from_json(index, ijson, &spec_version)?
                );
              }
              Ok(interactions)
            },
            _ => Ok(vec![])
        },
        None => Ok(vec![])
    }
}

fn determine_spec_version(file: &str, metadata: &BTreeMap<String, BTreeMap<String, String>>) -> PactSpecification {
  let specification = if metadata.contains_key("pact-specification") {
    metadata.get("pact-specification")
  } else {
    metadata.get("pactSpecification")
  };
  match specification {
    Some(spec) => {
      match spec.get("version") {
        Some(ver) => match lenient_semver::parse(ver) {
          Ok(ver) => match ver.major {
            1 => match ver.minor {
              0 => PactSpecification::V1,
              1 => PactSpecification::V1_1,
              _ => {
                log::warn!("Unsupported specification version '{}' found in the metadata in the pact file {:?}, will try load it as a V1 specification", ver, file);
                PactSpecification::V1
              }
            },
            2 => PactSpecification::V2,
            3 => PactSpecification::V3,
            4 => PactSpecification::V4,
            _ => {
                log::warn!("Unsupported specification version '{}' found in the metadata in the pact file {:?}, will try load it as a V3 specification", ver, file);
                PactSpecification::Unknown
            }
          },
          Err(err) => {
            log::warn!("Could not parse specification version '{}' found in the metadata in the pact file {:?}, assuming V3 specification - {}", ver, file, err);
            PactSpecification::Unknown
          }
        },
        None => {
          log::warn!("No specification version found in the metadata in the pact file {:?}, assuming V3 specification", file);
          PactSpecification::V3
        }
      }
    },
    None => {
      log::warn!("No metadata found in pact file {:?}, assuming V3 specification", file);
      PactSpecification::V3
    }
  }
}

impl RequestResponsePact {

    /// Returns the specification version of this pact
    pub fn spec_version(&self) -> PactSpecification {
        determine_spec_version(&s!("<Pact>"), &self.metadata)
    }

    /// Creates a `Pact` from a `Value` struct.
    pub fn from_json(file: &str, pact_json: &Value) -> anyhow::Result<RequestResponsePact> {
        let metadata = parse_meta_data(pact_json);
        let spec_version = determine_spec_version(file, &metadata);

        let consumer = match pact_json.get("consumer") {
            Some(v) => Consumer::from_json(v),
            None => Consumer { name: s!("consumer") }
        };
        let provider = match pact_json.get("provider") {
            Some(v) => Provider::from_json(v),
            None => Provider { name: s!("provider") }
        };
        Ok(RequestResponsePact {
            consumer,
            provider,
            interactions: parse_interactions(pact_json, spec_version.clone())?,
            metadata,
            specification_version: spec_version
        })
    }

    /// Creates a BTreeMap of the metadata of this pact.
    pub fn metadata_to_json(&self, pact_spec: &PactSpecification) -> BTreeMap<String, Value> {
        let mut md_map: BTreeMap<String, Value> = self.metadata.iter()
            .map(|(k, v)| {
                let key = match k.as_str() {
                  "pact-specification" => s!("pactSpecification"),
                  "pact-rust" => s!("pactRust"),
                  _ => k.clone()
                };
                (key, json!(v.iter()
                  .map(|(k, v)| (k.clone(), v.clone()))
                  .collect::<BTreeMap<String, String>>()))
            })
            .collect();

        md_map.insert(s!("pactSpecification"), json!({"version" : pact_spec.version_str()}));
        md_map.insert(s!("pactRust"), json!({"version" : s!(PACT_RUST_VERSION.unwrap_or("unknown"))}));
        md_map
    }

    /// Reads the pact file from a URL and parses the resulting JSON into a `Pact` struct
    pub fn from_url(url: &str, auth: &Option<HttpAuth>) -> anyhow::Result<RequestResponsePact> {
      let (url, json) = http_utils::fetch_json_from_url(&url.to_string(), auth)?;
      RequestResponsePact::from_json(&url, &json)
    }

    /// Returns a default RequestResponsePact struct
    pub fn default() -> RequestResponsePact {
      RequestResponsePact {
            consumer: Consumer { name: s!("default_consumer") },
            provider: Provider { name: s!("default_provider") },
            interactions: Vec::new(),
            metadata: RequestResponsePact::default_metadata(),
            specification_version: PactSpecification::V3
        }
    }

  /// Returns the default metadata
  pub fn default_metadata() -> BTreeMap<String, BTreeMap<String, String>> {
    btreemap!{
      s!("pact-specification") => btreemap!{ s!("version") => PactSpecification::V3.version_str() },
      s!("pact-rust") => btreemap!{ s!("version") => s!(PACT_RUST_VERSION.unwrap_or("unknown")) }
    }
  }
}

impl ReadWritePact for RequestResponsePact {
  fn read_pact(path: &Path) -> anyhow::Result<RequestResponsePact> {
    with_read_lock(path, 3, &mut |f| {
      let pact_json = serde_json::from_reader(f)
        .context("Failed to parse Pact JSON")?;
      RequestResponsePact::from_json(&format!("{:?}", path), &pact_json)
    })
  }

  fn merge(&self, pact: &dyn Pact) -> anyhow::Result<Box<dyn Pact>> {
    if self.consumer.name == pact.consumer().name && self.provider.name == pact.provider().name {
      let conflicts = iproduct!(self.interactions.clone(), pact.interactions().clone())
        .map(|i| i.0.conflicts_with(i.1))
        .filter(|conflicts| !conflicts.is_empty())
        .collect::<Vec<Vec<PactConflict>>>();
      let num_conflicts = conflicts.len();
      if num_conflicts > 0 {
        warn!("The following conflicting interactions where found:");
        for interaction_conflicts in conflicts {
          warn!(" Interaction '{}':", interaction_conflicts.first().unwrap().interaction);
          for conflict in interaction_conflicts {
            warn!("   {}", conflict.description);
          }
        }
        Err(anyhow!("Unable to merge pacts, as there were {} conflict(s) between the interactions. Please clean out your pact directory before running the tests.",
                    num_conflicts))
      } else {
        let interactions: Vec<Result<RequestResponseInteraction, String>> = self.interactions.iter()
          .merge_join_by(pact.interactions().iter(), |a, b| {
            let cmp = Ord::cmp(&a.provider_states.iter().map(|p| p.name.clone()).collect::<Vec<String>>(),
                               &b.provider_states().iter().map(|p| p.name.clone()).collect::<Vec<String>>());
            if cmp == Ordering::Equal {
              Ord::cmp(&a.description, &b.description())
            } else {
              cmp
            }
          })
          .map(|either| match either {
            Left(i) => Ok(i.clone()),
            Right(i) => i.as_request_response()
              .ok_or(format!("Can't convert interaction of type {} to V3 Synchronous/HTTP", i.type_of())),
            Both(_, i) => i.as_request_response()
              .ok_or(format!("Can't convert interaction of type {} to V3 Synchronous/HTTP", i.type_of()))
          })
          .collect();

        let errors: Vec<String> = interactions.iter()
          .filter(|i| i.is_err())
          .map(|i| i.as_ref().unwrap_err().to_string())
          .collect();
        if errors.is_empty() {
          Ok(Box::new(RequestResponsePact {
            provider: self.provider.clone(),
            consumer: self.consumer.clone(),
            interactions: interactions.iter()
              .filter(|i| i.is_ok())
              .map(|i| i.as_ref().unwrap().clone()).collect(),
            metadata: self.metadata.clone(),
            specification_version: self.specification_version.clone()
          }))
        } else {
          Err(anyhow!("Unable to merge pacts: {}", errors.join(", ")))
        }
      }
    } else {
      Err(anyhow!("Unable to merge pacts, as they have different consumers or providers"))
    }
  }

  fn default_file_name(&self) -> String {
    format!("{}-{}.json", self.consumer.name, self.provider.name)
  }
}

fn decode_query(query: &str) -> Result<String, String> {
  let mut chars = query.chars();
  let mut ch = chars.next();
  let mut buffer = vec![];

  while ch.is_some() {
    let c = ch.unwrap();
    trace!("ch = '{:?}'", ch);
    if c == '%' {
      let c1 = chars.next();
      let c2 = chars.next();
      match (c1, c2) {
        (Some(v1), Some(v2)) => {
          let mut s = String::new();
          s.push(v1);
          s.push(v2);
          let decoded: Result<Vec<u8>, _> = FromHex::from_hex(s.into_bytes());
          match decoded {
            Ok(n) => {
              trace!("decoded = '{:?}'", n);
              buffer.extend_from_slice(&n);
            },
            Err(err) => {
              error!("Failed to decode '%{}{}' to as HEX - {}", v1, v2, err);
              buffer.push('%' as u8);
              buffer.push(v1 as u8);
              buffer.push(v2 as u8);
            }
          }
        },
        (Some(v1), None) => {
          buffer.push('%' as u8);
          buffer.push(v1 as u8);
        },
        _ => buffer.push('%' as u8)
      }
    } else if c == '+' {
      buffer.push(' ' as u8);
    } else {
      buffer.push(c as u8);
    }

    ch = chars.next();
  }

  match str::from_utf8(&buffer) {
    Ok(s) => Ok(s.to_owned()),
    Err(err) => {
      error!("Failed to decode '{}' to UTF-8 - {}", query, err);
      Err(format!("Failed to decode '{}' to UTF-8 - {}", query, err))
    }
  }
}

fn encode_query(query: &str) -> String {
  query.chars().map(|ch| {
    match ch {
      ' ' => s!("+"),
      '-' => ch.to_string(),
      'a'..='z' => ch.to_string(),
      'A'..='Z' => ch.to_string(),
      '0'..='9' => ch.to_string(),
      _ => ch.escape_unicode()
          .filter(|u| u.is_digit(16))
          .batching(|it| {
              match it.next() {
                  None => None,
                  Some(x) => Some((x, it.next().unwrap()))
              }
          })
          .map(|u| format!("%{}{}", u.0, u.1))
          .collect()
    }
  }).collect()
}

/// Parses a query string into an optional map. The query parameter name will be mapped to
/// a list of values. Where the query parameter is repeated, the order of the values will be
/// preserved.
pub fn parse_query_string(query: &str) -> Option<HashMap<String, Vec<String>>> {
  if !query.is_empty() {
    Some(query.split('&').map(|kv| {
      trace!("kv = '{}'", kv);
      if kv.is_empty() {
        vec![]
      } else if kv.contains('=') {
        kv.splitn(2, '=').collect::<Vec<&str>>()
      } else {
        vec![kv]
      }
    }).fold(HashMap::new(), |mut map, name_value| {
      trace!("name_value = '{:?}'", name_value);
      if !name_value.is_empty() {
        let name = decode_query(name_value[0])
          .unwrap_or_else(|_| name_value[0].to_owned());
        let value = if name_value.len() > 1 {
          decode_query(name_value[1]).unwrap_or_else(|_| name_value[1].to_owned())
        } else {
          String::default()
        };
        trace!("decoded: '{}' => '{}'", name, value);
        map.entry(name).or_insert_with(|| vec![]).push(value);
      }
      map
    }))
  } else {
    None
  }
}

/// Converts the JSON struct into an HTTP Interaction
pub fn http_interaction_from_json(source: &str, json: &Value, spec: &PactSpecification)
-> anyhow::Result<Box<dyn Interaction + Send>> {
  match spec {
    PactSpecification::V4 => interaction_from_json(source, 0, json)
      .map(|i| i.boxed()),
    _ => Ok(Box::new(RequestResponseInteraction::from_json(0, json, spec)?))
  }
}

/// Converts the JSON struct into a Message Interaction
pub fn message_interaction_from_json(source: &str, json: &Value, spec: &PactSpecification) -> anyhow::Result<Box<dyn Interaction + Send>> {
  match spec {
    PactSpecification::V4 => interaction_from_json(source, 0, json)
      .map(|i| i.boxed()),
    _ => Message::from_json(0, json, spec).map(|i| i.boxed())
  }
}

/// Reads the pact file and parses the resulting JSON into a `Pact` struct
pub fn read_pact(file: &Path) -> anyhow::Result<Box<dyn Pact>> {
  let mut f = File::open(file)?;
  read_pact_from_file(&mut f, file)
}

/// Reads the pact from the file and parses the resulting JSON into a `Pact` struct
pub fn read_pact_from_file(file: &mut File, path: &Path) -> anyhow::Result<Box<dyn Pact>> {
  let buf = with_read_lock_for_open_file(path, file, 3, &mut |f| {
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    Ok(buf)
  })?;
  let pact_json = serde_json::from_str(&buf)
    .context("Failed to parse Pact JSON")
    .map_err(|err| {
      error!("read_pact_from_file: {}", err);
      debug!("read_pact_from_file: file contents = '{}'", buf);
      err
    })?;
  load_pact_from_json(&*path.to_string_lossy(), &pact_json)
    .map_err(|e| anyhow!(e))
}

/// Reads the pact file from a URL and parses the resulting JSON into a `Pact` struct
pub fn load_pact_from_url(url: &str, auth: &Option<HttpAuth>) -> anyhow::Result<Box<dyn Pact>> {
  let (url, pact_json) = http_utils::fetch_json_from_url(&url.to_string(), auth)?;
  load_pact_from_json(&url, &pact_json)
}

/// Loads a Pact model from a JSON Value
pub fn load_pact_from_json(source: &str, json: &Value) -> anyhow::Result<Box<dyn Pact>> {
  match json {
    Value::Object(map) => if map.contains_key("messages") {
      let pact = MessagePact::from_json(source, json)?;
      Ok(Box::new(pact))
    } else {
      let metadata = parse_meta_data(json);
      let spec_version = determine_spec_version(source, &metadata);
      match spec_version {
        PactSpecification::V4 => v4::from_json(&source, json),
        _ => Ok(Box::new(RequestResponsePact::from_json(source, json)?))
      }
    },
    _ => Err(anyhow!("Failed to parse Pact JSON from source '{}' - it is not a valid pact file", source))
  }
}

/// Trait for objects that can represent Pacts and can be read and written
pub trait ReadWritePact {
  /// Reads the pact file and parses the resulting JSON into a `Pact` struct
  fn read_pact(path: &Path) -> anyhow::Result<Self> where Self: std::marker::Sized;

  /// Merges this pact with the other pact, and returns a new Pact with the interactions sorted.
  /// Returns an error if there is a merge conflict, which will occur if the other pact is a different
  /// type, or if a V3 Pact then if any interaction has the
  /// same description and provider state and the requests and responses are different.
  fn merge(&self, other: &dyn Pact) -> anyhow::Result<Box<dyn Pact>>;

  /// Determines the default file name for the pact. This is based on the consumer and
  /// provider names.
  fn default_file_name(&self) -> String;
}

lazy_static!{
  static ref WRITE_LOCK: Mutex<()> = Mutex::new(());
}

/// Writes the pact out to the provided path. If there is an existing pact at the path, the two
/// pacts will be merged together unless overwrite is true. Returns an error if the file can not
/// be written or the pacts can not be merged.
pub fn write_pact(
  pact: Box<dyn Pact>,
  path: &Path,
  pact_spec: PactSpecification,
  overwrite: bool
) -> anyhow::Result<()> {
  fs::create_dir_all(path.parent().unwrap())?;
  let _lock = WRITE_LOCK.lock().unwrap();
  if !overwrite && path.exists() {
    debug!("Merging pact with file {:?}", path);
    let mut f = fs::OpenOptions::new().read(true).write(true).open(&path)?;
    let existing_pact = read_pact_from_file(&mut f, path)?;

    if existing_pact.specification_version() < pact.specification_version() {
      warn!("Note: Existing pact is an older specification version ({:?}), and will be upgraded",
            existing_pact.specification_version());
    }

    let merged_pact = pact.merge(existing_pact.borrow())?;
    let pact_json = serde_json::to_string_pretty(&merged_pact.to_json(pact_spec)?)?;

    with_write_lock(path, &mut f, 3, &mut |f| {
      f.set_len(0)?;
      f.seek(SeekFrom::Start(0))?;
      f.write_all(pact_json.as_bytes())?;
      Ok(())
    })
  } else {
    debug!("Writing new pact file to {:?}", path);
    let result = serde_json::to_string_pretty(&pact.to_json(pact_spec)?)?;
    let mut file = File::create(path)?;
    file.lock_exclusive()?;
    let result = file.write_all(result.as_bytes());
    file.unlock()?;
    result.map_err(|e| e.into())
  }
}

#[cfg(test)]
mod tests;
