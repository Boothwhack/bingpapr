pkgname=bingpapr
pkgver=0.1.2
pkgrel=1
pkgdesc="Bing daily wallpaper for Hyprpaper"
arch=('any')
url="https://github.com/Boothwhack/bingpapr"
license=('MIT')
depends=('bingdaily>=0.1.0')
makedepends=('cargo')
source=("$pkgname-$pkgver.tar.gz::https://github.com/Boothwhack/bingpapr/archive/refs/tags/$pkgname-v$pkgver.tar.gz")
sha256sums=('2c68cd6f035ab14a6c129ab36979a27ca12f0c033e101100d04c08fd11ed0715')

build() {
	cd "bingpapr-$pkgname-v$pkgver"

	cargo build --package $pkgname --release
}

package() {
	cd "bingpapr-$pkgname-v$pkgver"

	install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
}
