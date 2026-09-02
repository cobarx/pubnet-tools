//! Thin wrapper so `cargo run -p pubnetchk-android --bin uniffi-bindgen` drives
//! UniFFI's library-mode binding generator. See the crate README.
fn main() {
    uniffi::uniffi_bindgen_main()
}
