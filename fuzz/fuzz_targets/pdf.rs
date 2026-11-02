#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| okf_microservice::fuzz_input("pdf", data));
