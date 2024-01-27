pkgname=bingdaily
pkgver=0.1.1
pkgrel=1
pkgdesc="Minimal D-Bus service providing Bing's daily picture."
arch=('any')
url="https://github.com/Boothwhack/bingpapr"
license=('MIT')
makedepends=('cargo')
source=("$pkgname-$pkgver.tar.gz::https://github.com/Boothwhack/bingpapr/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('f9c46eb9df13d7c8a19173100a082fb6f19eeb099ddbcb0640c245d7ae48e522')

build() {
	cd "bingpapr-$pkgname-v$pkgver"

	cargo build --package $pkgname --release
}

package() {
	cd "bingpapr-$pkgname-v$pkgver"

	install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
	install -Dm644 "bingdaily/res/bliss.jpg" "$pkgdir/usr/lib/$pkgname/bliss.jpg"
	install -Dm644 "bingdaily/res/net.boothwhack.BingDaily1.service" "$pkgdir/usr/share/dbus-1/services/net.boothwhack.BingDaily1.service"
}
