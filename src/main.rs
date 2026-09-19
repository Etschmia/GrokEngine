fn main() {
    let mut args = std::env::args();
    let _ = args.next();
    if let Some(arg) = args.next() {
        if arg == "bench" {
            grokengine::bench::run();
            return;
        }
    }
    grokengine::uci::run();
}
