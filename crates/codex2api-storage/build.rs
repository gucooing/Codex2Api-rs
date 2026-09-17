fn main() {
    // SQLx embeds migration SQL at compile time; additions/removals must rebuild the crate.
    println!("cargo:rerun-if-changed=migrations");
}
