use crate::{control::socket, network::api, objects::read};


pub fn upload_all_reads(
    http_client: &reqwest::blocking::Client,
    api: &api::Api,
    reads: Vec<read::Read>
) -> (Vec<read::Read>, usize)
{
    let mut modified_reads: Vec<read::Read> = Vec::new();
    let mut err_count: usize = 0;
    // only upload in chunks of 50
    if reads.len() > 50 {
        //println!("Attempting to upload {} reads.", reads.len());
        // get the total number of full 50 count loops to do
        let num_loops = reads.len() / 50;
        let mut loop_counter = 0;
        // counter starts at 0, num_loops is at minimum 1
        // after the first loop counter is 1 and should exit if only 50 items
        while loop_counter < num_loops {
            let start_ix = loop_counter * 50;
            let slice = &reads[start_ix..start_ix+50];
            match socket::upload_reads(http_client, api, &slice) {
                Ok(count) => {
                    // if we uploaded the correct
                    if count == 50 {
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
                                read.status(),
                                read::READ_UPLOADED_TRUE
                            ))
                        }
                    } else {
                        println!("Error uploading reads. Count doesn't match. {} uploaded, expected {}", count, 50);
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
        let start_ix = loop_counter * 50;
        let slice = &reads[start_ix..reads.len()];
        match socket::upload_reads(http_client, api, &slice) {
            Ok(count) => {
                // Need to calculate the count... for 75 items (0-74)
                // only 1 loop, start_ix should be (1 * 50)
                // 75 - 50 = 25
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
                            read.status(),
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
                    let mut modified_reads: Vec<read::Read> = Vec::new();
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
                            read.status(),
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