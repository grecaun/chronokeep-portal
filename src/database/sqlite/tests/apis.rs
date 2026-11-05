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

//! Saved remote APIs.

use super::*;

#[test]
fn test_save_api() {
    let unique_path = "./test_save_api.sqlite";
    let original = api::Api::new(
        0,
        String::from("results-api"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
        String::from("random-token-value"),
        String::from("https:://example.com/"),
        false,
        false,
        String::from("ck01")
    );
    let mut sqlite = setup_tests(unique_path);
    let results = sqlite.save_api(&original);
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // test update functionality (nickname stays the same)
    let results = sqlite.save_api(
        &api::Api::new(
            0,
            String::from(original.nickname()),
            String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF),
            String::from("a-different-random-token"),
            String::from("https:://random.com/"),
            true,
            false,
            String::from("ck01")
        ));
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert_eq!(original.nickname(), first.nickname());
    assert_ne!(original.kind(), first.kind());
    assert_ne!(original.token(), first.token());
    assert_ne!(original.uri(), first.uri());
    // test update functionality (token and uri stays the same)
    // save original and verify it updated back
    _ = sqlite.save_api(&original);
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // save new entry
    let results = sqlite.save_api(&api::Api::new(
        0,
        String::from("new-nickname"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF),
        String::from(original.token()),
        String::from(original.uri()),
        false,
        false,
        String::from("ck01")
    ));
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert_ne!(original.nickname(), first.nickname());
    assert_ne!(original.kind(), first.kind());
    assert_eq!(original.token(), first.token());
    assert_eq!(original.uri(), first.uri());
    // attempt to save invalid type
    let result = sqlite.save_api(&api::Api::new(
        0,
        String::from("invalid_type_name"),
        String::from("invalid-type"),
        String::from("random-token"),
        String::from("https:://invalid-type.com/"),
        true,
        false,
        String::from("ck01")
    ));
    assert!(result.is_err());
    match result {
        Ok(_) => {
            println!("Expected an error...");
            panic!();
        },
        Err(DBError::DataInsertionError(_)) => println!("Expected error found!"),
        Err(e) => {
            println!("Unexpected error found... {}", e);
            panic!();
        }
    }
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_get_apis() {
    let unique_path = "./test_get_apis.sqlite";
    let original = api::Api::new(
        0,
        String::from("results-api"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
        String::from("random-token-value"),
        String::from("https:://example.com/"),
        false,
        false,
        String::from("ck01")
    );
    let mut sqlite = setup_tests(unique_path);
    _ = sqlite.save_api(&original);
    let result = sqlite.get_apis();
    assert!(result.is_ok());
    let apis = result.unwrap();
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // test that we can add a whole bunch of api entries and retrieve them
    for i in 0..5 {
        _ = sqlite.save_api(&api::Api::new(
            0,
            format!("api-{i}"),
            String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
            format!("token-number-10302031{i}"),
            String::from("https::api.chronokeep.com/"),
            true,
            false,
            String::from("ck01")
        ))
    }
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(6, apis.len());
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_delete_api() {
    let unique_path = "./test_delete_api.sqlite";
    let original = api::Api::new(
        0,
        String::from("results-api"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
        String::from("random-token-value"),
        String::from("https:://example.com/"),
        true,
        false,
        String::from("ck01")
    );
    let mut sqlite = setup_tests(unique_path);
    let orig_id = sqlite.save_api(&original).unwrap_or(0);
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let result = sqlite.delete_api(&orig_id);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(0, apis.len());
    let result = sqlite.delete_api(&orig_id);
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap());
    // verify that we only delete one from a list of apis
    let mut five_id: i64 = 0;
    for i in 0..10 {
        let tmp = sqlite.save_api(&api::Api::new(
            0,
            format!("results-api-{i}"),
            String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
            format!("random-token-value-{i}"),
            String::from("https:://example.com/"),
            true,
            false,
            String::from("ck01")
        ));
        if i == 5 {
            five_id = tmp.unwrap_or(-1);
        }
    }
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(10, apis.len());
    let result = sqlite.delete_api(&five_id);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(9, apis.len());
    drop(sqlite);
    finalize_tests(unique_path);
}
