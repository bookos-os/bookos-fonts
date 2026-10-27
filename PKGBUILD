pkgname=bookos-fonts
pkgver=0.1.0
pkgrel=1
pkgdesc="Fuentes de BookOS: ver, instalar, activar y quitar tipografías"
arch=('x86_64')
license=('LicenseRef-BookOS')
# gio: mandar a la papelera. fontconfig: fc-list, fc-query, fc-cache.
# vulkan-icd-loader: GPUI dibuja con wgpu sobre Vulkan.
depends=('fontconfig' 'glib2' 'libxkbcommon' 'wayland' 'vulkan-icd-loader')
makedepends=('rust' 'cargo')
source=()
options=('!strip' '!debug')

build() {
  cd "$startdir"
  cargo build --release --locked
}

package() {
  install -Dm755 "$startdir/target/release/bookos-fonts" "$pkgdir/usr/bin/bookos-fonts"
  install -Dm644 "$startdir/data/bookos-fonts.desktop" \
    "$pkgdir/usr/share/applications/bookos-fonts.desktop"
  install -Dm644 "$startdir/data/bookos-fonts.svg" \
    "$pkgdir/usr/share/icons/hicolor/scalable/apps/bookos-fonts.svg"
  install -Dm644 "$startdir/data/bookos-fonts.png" \
    "$pkgdir/usr/share/icons/hicolor/512x512/apps/bookos-fonts.png"
}
