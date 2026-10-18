prefix  := env_var_or_default("PREFIX", "/usr/local")
destdir := env_var_or_default("DESTDIR", "")
bindir  := env_var_or_default("BINDIR", prefix + "/bin")
mandir  := env_var_or_default("MANDIR", prefix + "/share/man")

default:
    @just --list

# Generate the man pages.
man:
    cargo run --locked --features man -- man docs/man/generated
    pandoc -s -f gfm -t man --shift-heading-level-by=-1 \
        -V section=5 -V header="blinools manual" -V footer=blinools \
        --metadata date="$(date -u +%F)" \
        docs/configuration.md -o docs/man/generated/blinools.toml.5
    sed -i -E 's/^\.SH (.*)$/.SH \U\1/' docs/man/generated/blinools.toml.5

# Generate the Markdown CLI reference in docs/cli from the man pages.
cli-docs: man
    mkdir -p docs/cli
    for page in docs/man/generated/*.1; do \
        sed 's/\\\*(Aq/\x27/g' "$page" | pandoc -f man -t gfm --wrap=none \
            | sed -E -e 's/(\*\*)?blinools\.toml(\*\*)?\(5\)/[blinools.toml(5)](..\/configuration.md)/g' \
                     -e 's/\b(blinools(-[a-z]+)*)\(1\)/[\1(1)](\1.md)/g' \
            > "docs/cli/$(basename "$page" .1).md"; \
    done

# Check the code, man pages and CLI reference.
check *args: man cli-docs
    git diff --exit-code -- docs/cli
    test -z "$(git ls-files --others --exclude-standard -- docs/cli)"
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

clean:
    cargo clean
    rm -rf docs/man/generated
    rm -rf docs/cli
