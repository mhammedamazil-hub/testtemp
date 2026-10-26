# Hand-written Meson build for libXft 2.3.2 (upstream ships autotools only).
project('libXft', 'c', version: '2.3.2',
  default_options: ['c_std=gnu11', 'warning_level=1'])

cc = meson.get_compiler('c')
dep_x11        = dependency('x11')
dep_xrender    = dependency('xrender', version: '>= 0.8.2')
dep_freetype   = dependency('freetype2', version: '>= 2.1.6')
dep_fontconfig = dependency('fontconfig', version: '>= 2.5.92')
dep_xproto     = dependency('xproto')

conf = configuration_data()
conf.set('HAVE_CONFIG_H', 1)
conf.set('HAVE_STDLIB_H', 1)
conf.set('HAVE_STRING_H', 1)
conf.set('HAVE_STRINGS_H', 1)
configure_file(output: 'config.h', configuration: conf)

extra_inc = include_directories('include', 'include/X11/Xft', '.')

srcs = [
  'src/xftcolor.c', 'src/xftcore.c', 'src/xftdbg.c', 'src/xftdpy.c',
  'src/xftdraw.c', 'src/xftextent.c', 'src/xftfont.c', 'src/xftfreetype.c',
  'src/xftglyphs.c', 'src/xftinit.c', 'src/xftlist.c', 'src/xftname.c',
  'src/xftrender.c', 'src/xftstr.c', 'src/xftswap.c', 'src/xftxlfd.c',
]

libxft = shared_library('Xft', srcs,
  include_directories: extra_inc,
  dependencies: [dep_x11, dep_xrender, dep_freetype, dep_fontconfig, dep_xproto],
  version: '2.3.2', soversion: '2',
  install: true)

install_headers('include/X11/Xft/Xft.h', 'include/X11/Xft/XftCompat.h',
  subdir: 'X11/Xft')

pkg = import('pkgconfig')
pkg.generate(libxft,
  name: 'Xft', description: 'X FreeType library',
  requires: ['xproto'],
  requires_private: ['xrender', 'fontconfig', 'freetype2'])
