prefix  := env_var_or_default("PREFIX", "/usr/local")
destdir := env_var_or_default("DESTDIR", "")
bindir  := env_var_or_default("BINDIR", prefix + "/bin")
mandir  := env_var_or_default("MANDIR", prefix + "/share/man")

default:
    @just --list

# Generate the man pages.
man:
    rm -r docs/man/generated || true
    cargo run --features man -- man docs/man/generated
    scdoc < docs/man/blinools.toml.5.scd > docs/man/generated/blinools.toml.5

# Check the code and man pages.
check *args: man
    if ! git diff --exit-code -- 'docs/man/generated/*' \
       || [ -n "$(git ls-files --others --exclude-standard -- 'docs/man/generated/*')" ]; then \
        echo "Run 'just man' and commit the updated man pages." >&2; \
        exit 1; \
    fi
    cargo fmt --all -- --check
    cargo clippy --locked --all-targets --features man {{args}}
    cargo test --locked {{args}}
    mandoc -Tlint -Werror docs/man/generated/*

# Build the release binary.
build:
    cargo build --locked --release --bin blinools

# Generate man pages, build, and install (honors PREFIX, BINDIR, MANDIR, DESTDIR).
install: man build
    install -d "{{destdir}}{{bindir}}" "{{destdir}}{{mandir}}/man1" "{{destdir}}{{mandir}}/man5"
    install -m755 target/release/blinools "{{destdir}}{{bindir}}/blinools"
    install -m644 docs/man/generated/*.1 "{{destdir}}{{mandir}}/man1/"
    install -m644 docs/man/generated/*.5 "{{destdir}}{{mandir}}/man5/"

# Remove installed files.
uninstall:
    rm -f "{{destdir}}{{bindir}}/blinools"
    for f in docs/man/generated/*.1; do rm -f "{{destdir}}{{mandir}}/man1/$(basename "$f")"; done
    for f in docs/man/generated/*.5; do rm -f "{{destdir}}{{mandir}}/man5/$(basename "$f")"; done
