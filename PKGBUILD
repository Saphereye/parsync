pkgname=parsync-git
pkgver=0.2.3.r6.a96a8ae
pkgrel=1
pkgdesc="A parallel file synchronization tool written in Rust"
arch=('x86_64')
url="https://github.com/Saphereye/parsync"
license=('GPL')
depends=()
makedepends=('rust' 'cargo' 'git')
provides=('parsync')
conflicts=('parsync')
source=("git+https://github.com/Saphereye/parsync.git")
sha256sums=('SKIP')

pkgver() {
    cd parsync
    # Get version from Cargo.toml
    _ver=$(grep '^version =' Cargo.toml | head -1 | cut -d'"' -f2)
    # Get git info
    _rev=$(git rev-list --count HEAD)
    _hash=$(git rev-parse --short HEAD)
    # Combine: 0.1.0.r123.abc1234
    printf "%s.r%s.%s" "$_ver" "$_rev" "$_hash"
}

build() {
    cd parsync
    cargo build --release --locked --all-features
}

package() {
    cd parsync
    install -Dm755 "target/release/parsync" "$pkgdir/usr/bin/parsync"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/parsync/LICENSE"
    install -Dm644 README.md "$pkgdir/usr/share/doc/parsync/README.md"
}
