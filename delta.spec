Name:           delta
Version:        0.18.2
Release:        6%{?dist}
Summary:        A syntax-highlighting pager for git, diff, and grep output
URL:            https://github.com/dandavison/delta
License:        MIT
Source0:        https://github.com/dandavison/delta/archive/refs/tags/%{version}.tar.gz

# BuildRequires: List all packages required to build the software
BuildRequires:  git
BuildRequires:  python3
BuildRequires:  curl
BuildRequires:  gcc
BuildRequires:  upx

%define debug_package %{nil}

%description
Code evolves, and we all spend time studying diffs. Delta aims to make this both efficient and enjoyable:
it allows you to make extensive changes to the layout and styling of diffs, as well as allowing you to
stay arbitrarily close to the default git/diff output.

%prep
%setup -q

%build
# Install Rust using curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
export PATH="$PATH:$HOME/.cargo/bin"
$HOME/.cargo/bin/cargo build --release --all-features
upx target/release/%{name}

%install
# You may need to adjust paths and permissions as necessary
install -D -m 755 target/release/%{name} %{buildroot}/usr/bin/%{name}
install -D -m 644 LICENSE %{buildroot}/usr/share/licenses/%{name}/LICENSE
install -D -m 644 README.md %{buildroot}/usr/share/doc/%{name}/README.md

%check
$HOME/.cargo/bin/cargo test --release

%files
# List all installed files and directories
%license LICENSE
%doc README.md
/usr/bin/%{name}

%changelog
* Sun Jan 11 2026 - Danie de Jager - 0.18.2-6
* Mon Oct 20 2025 - Danie de Jager - 0.18.2-5
* Mon Jun 16 2025 - Danie de Jager - 0.18.2-4
* Tue Feb 4 2025 - Danie de Jager - 0.18.2-3
* Thu Nov 14 2024 - Danie de Jager - 0.18.2-2
* Thu Sep 1 2024 - Danie de Jager - 0.18.2-1
* Sun Aug 25 2024 - Danie de Jager - 0.18.1-1
* Tue Jul 9 2024 - Danie de Jager - 0.17.0-2
* Mon May 13 2024 - Danie de Jager - 0.17.0-1
- Initial version
