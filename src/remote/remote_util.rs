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

use std::sync::{Arc, Mutex};
use std::time::Duration;
use reqwest::header::{HeaderMap, HeaderValue, CONNECTION};

use crate::{control::socket, database::{sqlite, Database}, network::api, objects::read};

pub(crate) const NUMBER_READS_PER_UPLOAD: usize = 100;

/// HTTP client that doesn't keep connections open between requests.
pub fn http_client(timeout: Duration) -> reqwest::Result<reqwest::blocking::Client> {
    let mut headers = HeaderMap::new();
    // Ask the server to close its end once it has responded.
    headers.insert(CONNECTION, HeaderValue::from_static("close"));
    reqwest::blocking::ClientBuilder::new()
        .timeout(timeout)
        .connect_timeout(timeout)
        // Keep no idle connections: each one closes once its response is read.
        .pool_max_idle_per_host(0)
        .default_headers(headers)
        .build()
}

pub fn upload_all_reads(
    http_client: &reqwest::blocking::Client,
    api: &api::Api,
    reads: Vec<read::Read>
) -> (Vec<read::Read>, usize)
{
    let mut modified_reads: Vec<read::Read> = Vec::new();
    let mut err_count: usize = 0;
    // only upload in chunks of NUMBER_READS_PER_UPLOAD
    if reads.len() > NUMBER_READS_PER_UPLOAD {
        //println!("Attempting to upload {} reads.", reads.len());
        // get the total number of full NUMBER_READS_PER_UPLOAD count loops to do
        let num_loops = reads.len() / NUMBER_READS_PER_UPLOAD;
        let mut loop_counter = 0;
        // counter starts at 0, num_loops is at minimum 1
        // after the first loop counter is 1 and should exit if only NUMBER_READS_PER_UPLOAD items
        while loop_counter < num_loops {
            let start_ix = loop_counter * NUMBER_READS_PER_UPLOAD;
            let slice = &reads[start_ix..start_ix+NUMBER_READS_PER_UPLOAD];
            match socket::upload_reads(http_client, api, &slice) {
                Ok(count) => {
                    // if we uploaded the correct
                    if count == NUMBER_READS_PER_UPLOAD {
                        for read in slice {
                            modified_reads.push(read::Read::new(
                                read.id(),
                                String::from(read.chip()),
                                read.seconds(),
                                read.milliseconds(),
                                read.reader_seconds(),
                                read.reader_milliseconds(),
                                read.antenna(),
                                String::from(read.reader()),
                                String::from(read.rssi()),
                                read::READ_UPLOADED_TRUE
                            ))
                        }
                    } else {
                        println!("Error uploading reads. Count doesn't match. {} uploaded, expected {}", count, NUMBER_READS_PER_UPLOAD);
                        err_count += 1;
                    }
                },
                Err(e) => {
                    println!("Error uploading reads: {:?}", e);
                    err_count += 1;
                }
            }
            loop_counter = loop_counter + 1;
        }
        let start_ix = loop_counter * NUMBER_READS_PER_UPLOAD;
        let slice = &reads[start_ix..reads.len()];
        // When the reads fill whole batches there's nothing left; don't send an empty request.
        let leftover = if slice.is_empty() { Ok(0) } else { socket::upload_reads(http_client, api, &slice) };
        match leftover {
            Ok(count) => {
                // Need to calculate the count... for 75 items (0-74)
                // only 1 loop, start_ix should be (1 * NUMBER_READS_PER_UPLOAD)
                // EX if NUMBER_READS_PER_UPLOAD is 50 : 75 - 50 = 25
                let amt = reads.len() - start_ix;
                // check for correct amout
                if count == amt {
                    for read in slice {
                        modified_reads.push(read::Read::new(
                            read.id(),
                            String::from(read.chip()),
                            read.seconds(),
                            read.milliseconds(),
                            read.reader_seconds(),
                            read.reader_milliseconds(),
                            read.antenna(),
                            String::from(read.reader()),
                            String::from(read.rssi()),
                            read::READ_UPLOADED_TRUE
                        ));
                    }
                } else {
                    println!("Error uploading reads. Count doesn't match. {} uploaded, expected {}", count, amt);
                    err_count += 1;
                }
            },
            Err(e) => {
                println!("Error uploading reads: {:?}", e);
                err_count += 1;
            }
        }
    } else if reads.len() > 0 {
        //println!("Attempting to upload {} reads.", reads.len());
        match socket::upload_reads(http_client, api, &reads) {
            Ok(count) => {
                // if we uploaded the correct
                if count == reads.len() {
                    for read in reads {
                        modified_reads.push(read::Read::new(
                            read.id(),
                            String::from(read.chip()),
                            read.seconds(),
                            read.milliseconds(),
                            read.reader_seconds(),
                            read.reader_milliseconds(),
                            read.antenna(),
                            String::from(read.reader()),
                            String::from(read.rssi()),
                            read::READ_UPLOADED_TRUE
                        ));
                    }
                } else {
                    println!("Error uploading reads. Count doesn't match. {} uploaded, expected {}", count, reads.len());
                    err_count += 1;
                }
            },
            Err(e) => {
                println!("Error uploading reads: {:?}", e);
                err_count += 1;
            }
        }
    }
    return (modified_reads, err_count);
}

/// Uploads every saved read to each enabled Chronokeep Remote API (the screen's
/// Manual Upload). The database is locked only to load the reads and to record
/// which were uploaded, never during the upload, and no other lock is needed, so
/// readers and everything else keep running while it uploads. Returns the number
/// of failed upload requests.
pub fn upload_saved_reads(
    http_client: &reqwest::blocking::Client,
    sqlite: &Arc<Mutex<sqlite::SQLite>>,
) -> usize {
    let (apis, reads) = {
        let Ok(sq) = sqlite.lock() else { return 0 };
        let apis: Vec<api::Api> = match sq.get_apis() {
            Ok(apis) => apis.into_iter().filter(|a| {
                (a.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || a.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF) && a.enabled()
            }).collect(),
            Err(e) => {
                println!("error getting apis: {e}");
                return 0;
            }
        };
        if apis.is_empty() {
            return 0;
        }
        match sq.get_all_reads() {
            Ok(reads) => (apis, reads),
            Err(e) => {
                println!("Error geting reads to upload. {e}");
                return 0;
            }
        }
    };
    if reads.is_empty() {
        return 0;
    }
    let mut errors = 0;
    for api in &apis {
        println!("Uploading {} reads to {} at {}", reads.len(), api.nickname(), api.uri());
        let (modified_reads, e_count) = upload_all_reads(http_client, api, reads.clone());
        println!("Uploaded {} reads to {} at {}. Error count: {}", modified_reads.len(), api.nickname(), api.uri(), e_count);
        errors += e_count;
        if let Ok(mut sq) = sqlite.lock() {
            if let Err(e) = sq.update_reads_status(&modified_reads) {
                println!("Error updating uploaded reads: {e}");
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests;
