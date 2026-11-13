project('orbit-c', 'c', version: '0.1.2', meson_version: '>=1.1', default_options: ['c_std=c11'])

cc = meson.get_compiler('c')
native_dir = get_option('native_dir')
if not native_dir.startswith('/')
  native_dir = meson.project_source_root() / native_dir
endif
native = cc.find_library('claviron_orbit', dirs: native_dir, required: true)
inc = include_directories('include')

executable('orbit-echo', 'examples/echo.c', include_directories: inc,
  dependencies: [native, dependency('threads')], build_rpath: native_dir)
executable('orbit-echo-low-level', 'examples/echo-low-level.c', include_directories: inc,
  dependencies: [native, dependency('threads')], build_rpath: native_dir)
executable('orbit-abi-check', 'examples/abi-check.c', include_directories: inc,
  dependencies: native, build_rpath: native_dir)
executable('orbit-runtime-check', 'examples/runtime-check.c', include_directories: inc,
  dependencies: native, build_rpath: native_dir)
executable('orbit-dispatcher-check', 'examples/dispatcher-check.c', include_directories: inc,
  dependencies: [native, dependency('threads')], build_rpath: native_dir)

install_headers('include/orbit.h', subdir: 'claviron')
foreach candidate : ['libclaviron_orbit.so', 'libclaviron_orbit.dylib', 'libclaviron_orbit.a']
  fs = import('fs')
  if fs.is_file(native_dir / candidate)
    install_data(native_dir / candidate, install_dir: get_option('libdir'))
  endif
endforeach
install_data('LICENSE-MIT', 'LICENSE-APACHE', install_dir: get_option('datadir') / 'licenses' / 'orbit-c')

pkg = import('pkgconfig')
pkg.generate(name: 'Orbit C', filebase: 'orbit-c', version: meson.project_version(),
  description: 'C ABI for Orbit HTTP and message-session applications',
  libraries: ['-L${libdir}', '-lclaviron_orbit'], subdirs: 'claviron')
