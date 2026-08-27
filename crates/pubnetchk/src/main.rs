fn main() {
    // Capture the machine's local UTC offset while we're still single-threaded.
    // The `time` crate refuses to read the local offset once tokio spawns worker
    // threads (concurrent env access is unsound), so this must run before the
    // runtime is built. The HTML report's footer uses it to show local time.
    pubnet_tools::output::html::init_local_offset();

    let runtime = tokio::runtime::Runtime::new().expect("failed to build the tokio runtime");
    runtime.block_on(pubnet_tools::cli::run());
}
