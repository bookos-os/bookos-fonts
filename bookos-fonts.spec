# El binario se compila antes con cargo (empaquetar.sh): rpmbuild solo coloca
# los archivos. Compilar dentro de rpmbuild obligaría a vendorizar todo el
# árbol de crates para que funcione sin red.
%global debug_package %{nil}

Name:           bookos-fonts
Version:        0.1.0
Release:        1%{?dist}
Summary:        Fuentes de BookOS: ver, instalar, activar y quitar tipografías
License:        LicenseRef-BookOS
# fc-list, fc-query y fc-cache; gio para la papelera; wgpu abre Vulkan en
# tiempo de ejecución, así que el generador de dependencias no lo ve.
Requires:       fontconfig
Requires:       glib2
Requires:       vulkan-loader

%description
Gestor de fuentes nativo de BookOS hecho con GPUI.

%install
install -Dm755 %{raiz}/target/release/bookos-fonts %{buildroot}%{_bindir}/bookos-fonts
install -Dm644 %{raiz}/data/bookos-fonts.desktop %{buildroot}%{_datadir}/applications/bookos-fonts.desktop
install -Dm644 %{raiz}/data/bookos-fonts.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/bookos-fonts.svg
install -Dm644 %{raiz}/data/bookos-fonts.png %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/bookos-fonts.png

%files
%{_bindir}/bookos-fonts
%{_datadir}/applications/bookos-fonts.desktop
%{_datadir}/icons/hicolor/scalable/apps/bookos-fonts.svg
%{_datadir}/icons/hicolor/512x512/apps/bookos-fonts.png
