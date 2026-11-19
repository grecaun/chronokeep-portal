/*
Chronokeep Portal - RFID Tag Reading Software
Copyright (C) 2026 James Sentinella

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Saving the list of APIs a client sends.

use std::fs;

use crate::control::socket::{errors::Errors, save_api_list};
use crate::database::{sqlite::tests::setup_tests, Database};
use crate::network::api::{self, Api};

fn remote(id: i64, name: &str, kind: &str, uri: &str) -> Api {
    Api::new(id, String::from(name), String::from(kind), format!("key-{name}"), String::from(uri), true, true, String::new())
}

/// The saved APIs as (name, uri), sorted by name.
fn saved(sq: &crate::database::sqlite::SQLite) -> Vec<(String, String)> {
    let mut apis: Vec<(String, String)> = sq.get_apis().unwrap().iter()
        .map(|a| (String::from(a.nickname()), String::from(a.uri())))
        .collect();
    apis.sort();
    apis
}

#[test]
fn saves_several_remote_apis_at_once() {
    let path = "./test_api_save_several.sqlite";
    let mut sq = setup_tests(path);
    save_api_list(&mut sq, vec![
        remote(0, "a", api::API_TYPE_CHRONOKEEP_REMOTE, "https://remote.chronokeep.com/"),
        remote(0, "b", api::API_TYPE_CHRONOKEEP_REMOTE_SELF, "https://remote.example.com/"),
        remote(0, "c", api::API_TYPE_CHRONOKEEP_REMOTE_SELF, "http://192.168.1.10:8181/"),
    ]).unwrap();
    assert_eq!(saved(&sq).len(), 3);
    drop(sq);
    _ = fs::remove_file(path);
}

#[test]
fn adds_remote_apis_alongside_one_already_saved() {
    let path = "./test_api_save_alongside.sqlite";
    let mut sq = setup_tests(path);
    let first = sq.save_api(&remote(0, "a", api::API_TYPE_CHRONOKEEP_REMOTE, "https://remote.chronokeep.com/")).unwrap();
    // a client sends the whole list back: the saved one plus two new ones
    save_api_list(&mut sq, vec![
        remote(first, "a", api::API_TYPE_CHRONOKEEP_REMOTE, "https://remote.chronokeep.com/"),
        remote(0, "b", api::API_TYPE_CHRONOKEEP_REMOTE_SELF, "https://remote.example.com/"),
        remote(0, "c", api::API_TYPE_CHRONOKEEP_REMOTE_SELF, "http://192.168.1.10:8181/"),
    ]).unwrap();
    let names: Vec<String> = saved(&sq).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["a", "b", "c"]);
    drop(sq);
    _ = fs::remove_file(path);
}

#[test]
fn every_uri_is_saved_ending_in_a_slash() {
    let path = "./test_api_save_slash.sqlite";
    let mut sq = setup_tests(path);
    save_api_list(&mut sq, vec![
        remote(0, "a", api::API_TYPE_CHRONOKEEP_REMOTE, "https://remote.chronokeep.com"),
        remote(0, "b", api::API_TYPE_CHRONOKEEP_REMOTE_SELF, "https://remote.example.com/"),
    ]).unwrap();
    assert_eq!(saved(&sq), [
        (String::from("a"), String::from("https://remote.chronokeep.com/")),
        (String::from("b"), String::from("https://remote.example.com/")),
    ]);
    drop(sq);
    _ = fs::remove_file(path);
}

#[test]
fn saves_nothing_if_any_api_is_another_kind() {
    let path = "./test_api_save_invalid.sqlite";
    let mut sq = setup_tests(path);
    let result = save_api_list(&mut sq, vec![
        remote(0, "a", api::API_TYPE_CHRONOKEEP_REMOTE, "https://remote.chronokeep.com/"),
        remote(0, "b", "CHRONOKEEP_RESULTS", "https://api.chronokeep.com/"),
    ]);
    assert!(matches!(result, Err(Errors::InvalidApiType { .. })), "{result:?}");
    assert!(saved(&sq).is_empty());
    drop(sq);
    _ = fs::remove_file(path);
}
