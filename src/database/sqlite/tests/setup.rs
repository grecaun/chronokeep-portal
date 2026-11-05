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

//! Creating the database and upgrading older ones.

use super::*;

#[test]
fn test_setup() {
    let unique_path = "./test_setup.sqlite";
    {
        let new_conn = rusqlite::Connection::open(unique_path);
        assert!(new_conn.is_ok());
        let mut sqlite = SQLite {
            conn: new_conn.unwrap()
        };
        let res = sqlite.setup();
        match res {
            Ok(_) => println!("Everything went ok!"),
            Err(e) => {
                println!("Something went wrong! {}", e.to_string());
                panic!();
            }
        }
    }
    finalize_tests(unique_path);
}

#[test]
fn test_update() {
    let unique_path = "./test_update.sqlite";
    {
        let mut sqlite = setup_v1(unique_path);
        match sqlite.update(1, 1) {
            Ok(_) => println!("Everything went ok!"),
            Err(e) => {
                println!("Something went wrong! {}", e);
                panic!();
            }
        }
    }
    finalize_tests(unique_path);
}
