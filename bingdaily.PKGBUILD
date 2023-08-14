pkgname=bingdaily
pkgver=0.1.1
pkgrel=1
pkgdesc="Minimal D-Bus service providing Bing's daily picture."
arch=('any')
url="https://github.com/Boothwhack/bingpapr"
license=('MIT')
makedepends=('cargo')
source=("$pkgname-$pkgver.tar.gz::https://github.com/Boothwhack/bingpapr/archive/refs/tags/$pkgname-v$pkgver.tar.gz")
sha256sums=('c60585c0df6f0bfeeb4fc64e5fc46e6a9f13344931a737c343e86dfe42398cd7')

build() {
	cd "bingpapr-$pkgname-v$pkgver"

	cargo build --package bingdaily --release
}

package() {
	cd "bingpapr-$pkgname-v$pkgver"

	install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
	install -Dm644 "bingdaily/res/bliss.jpg" "$pkgdir/usr/lib/$pkgname/bliss.jpg"
	install -Dm644 "bingdaily/res/net.boothwhack.BingDaily1.service" "$pkgdir/usr/share/dbus-1/services/net.boothwhack.BingDaily1.service"
}
