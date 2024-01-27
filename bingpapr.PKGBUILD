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
sha256sums=('c333a7b2643b46a6cdcf3d433aa259c1f95e9d519c46db1b4fe931899317517d')

build() {
	cd "bingpapr-$pkgname-v$pkgver"

	cargo build --package $pkgname --release
}

package() {
	cd "bingpapr-$pkgname-v$pkgver"

	install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
}
