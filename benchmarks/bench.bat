@echo off
setlocal
set RUSTFLAGS=-C target-feature=+bmi1
cargo bench %*
endlocal