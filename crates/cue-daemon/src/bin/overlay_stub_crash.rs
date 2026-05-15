//! Stub that exits immediately with non-zero status.
//! Used to test crash-loop cap exhaustion in the overlay supervisor.

fn main() {
    std::process::exit(1);
}
