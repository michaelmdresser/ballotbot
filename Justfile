# Make sure to cargo install sqlx-cli first!
test:
    cargo install sqlx-cli
    DATABASE_URL="sqlite://test.db" sqlx database setup
    DATABASE_URL="sqlite://test.db" sqlx migrate run
    DATABASE_URL="sqlite://test.db" cargo test
run:
    DATABASE_URL="sqlite://prod.db" sqlx database setup
    DATABASE_URL="sqlite://prod.db" sqlx migrate run
    RUST_BACKTRACE=1 DATABASE_URL="sqlite://prod.db" cargo run
