%bcond check 1

Name:           blinools
Version:        0.4.0
Release:        1%{?dist}
Summary:        Collection of blinxen's command-line utilities
License:        GPL-3.0-or-later AND (0BSD OR Apache-2.0 OR MIT) AND Apache-2.0 AND BSL-1.0 AND MIT AND Unicode-3.0 AND Unlicense AND Zlib
URL:            https://github.com/blinxen/%{name}
Source0:        %{url}/archive/%{version}/%{name}-%{version}.tar.gz
# Crates are vendored because garde and nftables are not packaged in Fedora.
# Created with:
#   cargo vendor --locked vendor
#   tar caf ../blinools-0.4.0-vendor.tar.xz vendor
Source1:        %{name}-%{version}-vendor.tar.xz

BuildRequires:  cargo-rpm-macros >= 24
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  pandoc

Requires:       passt
Requires:       virtiofsd

%description
A collection of bash scripts converted into a Rust CLI.

%files
%license LICENSE
%license LICENSE.dependencies
%license cargo-vendor.txt
%{_bindir}/blinools
%{_mandir}/man1/blinools*.1*
%{_mandir}/man5/blinools.toml.5*
%{_docdir}/%{name}/

%prep
%autosetup -n %{name}-%{version_no_tilde} -p1 -a1
%cargo_prep -v vendor

%build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%{cargo_vendor_manifest}

# Build with the "man" feature only to generate the man pages
%cargo_build -f man
mkdir -p docs/man/generated
target/rpm/%{name} man docs/man/generated
pandoc -s -f gfm -t man --shift-heading-level-by=-1 \
    -V section=5 -V header="blinools manual" -V footer=blinools \
    --metadata date="$(date -u +%%F)" \
    docs/configuration.md -o docs/man/generated/blinools.toml.5
sed -i -E 's/^\.SH (.*)$/.SH \U\1/' docs/man/generated/blinools.toml.5

# Rebuild the shipped binary without the "man" feature
%cargo_build

%install
%cargo_install

# Man pages
install -Dpm 0644 -t %{buildroot}%{_mandir}/man1 docs/man/generated/*.1
install -Dpm 0644 -t %{buildroot}%{_mandir}/man5 docs/man/generated/*.5

# Documentation
install -Dpm 0644 -t %{buildroot}%{_docdir}/%{name} README.md docs/*.md
install -Dpm 0644 -t %{buildroot}%{_docdir}/%{name}/cli docs/cli/*.md

%if %{with check}
%check
# These tests spawn processes into new PID/mount namespaces and mount a fresh
# /proc, which is not permitted inside mock/Copr build chroots.
%cargo_test -- -- \
    --skip sandbox::process::tests::a_failing_pre_exec_hook_fails_spawn \
    --skip sandbox::process::tests::a_missing_binary_fails_spawn \
    --skip sandbox::process::tests::an_isolated_process_waits_for_exec \
    --skip sandbox::process::tests::the_exit_status_is_reported \
    --skip sandbox::process::tests::the_process_is_pid_one_in_its_namespace \
    --skip sandbox::process::tests::stdout_is_redirected \
    --skip sandbox::process::tests::waiting_fails_when_the_process_exits_early
%endif

%changelog
* Thu Oct 22 2026 blinxen <h-k-81@hotmail.com> - 0.4.0-1
- Initial package
