fn main() {
    use std::io::Write;
    if ctrlc::set_handler(|| {
        revayat_core::process_control::request_cancel();
        subsync_rs::request_cancel();
    })
    .is_err()
    {
        let _ = writeln!(
            std::io::stderr().lock(),
            "ERROR: Could not install cancellation handler."
        );
        std::process::exit(2);
    }
    let status = revayat_cli::run();
    std::process::exit(if revayat_core::process_control::cancel_requested() {
        130
    } else {
        status
    });
}
