fn main() {
    // `sqlx::migrate!` embeds the directory; rebuild when a migration is added or changed.
    println!("cargo:rerun-if-changed=migrations");
}
