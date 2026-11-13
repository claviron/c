if(VCPKG_TARGET_IS_OSX)
    set(system macos)
    set(shared_extension dylib)
elseif(VCPKG_TARGET_IS_LINUX)
    set(system linux)
    set(shared_extension so)
elseif(VCPKG_TARGET_IS_FREEBSD)
    set(system freebsd)
    set(shared_extension so)
else()
    message(FATAL_ERROR "Orbit C has no release archive for ${TARGET_TRIPLET}")
endif()

if(VCPKG_TARGET_ARCHITECTURE STREQUAL "x64")
    set(arch x86_64)
elseif(VCPKG_TARGET_ARCHITECTURE STREQUAL "arm64")
    set(arch aarch64)
else()
    message(FATAL_ERROR "Orbit C has no release archive for ${TARGET_TRIPLET}")
endif()

set(sha512_freebsd_aarch64 35d1d31f8ae8603f07ecdffe402ecd1d3abef76510b1f4e7267d5ca0d0a61f97c26f85e37befffb0f18016bc978f3e76d05c2ec003b3242e052d1dc41cf7a6e5)
set(sha512_freebsd_x86_64 37b7afd32207f38ba019e355da75190dfc52e93969ee8bb6447a01b89799c8ee53e86ec303a10da1aaade50a0a75839e37ff23d88ad87198c9d0129a8398ab30)
set(sha512_linux_aarch64 0e4cc3683b9e01127e1902500baa874240c1f8a2435aabc28870a9e6e91751611809b21c9d11c4577db9526f0372d7e82c8ca2db05d3a9c03102f735be30fae5)
set(sha512_linux_x86_64 52f90f5e3a5193f62dfa3f4f3e548029f9f770f84a5f87026d522c304c4dbb56ee6c543d57d3681d2b2c99314ea4c64cceb506729c629f50320e9a7b8cfb571c)
set(sha512_macos_aarch64 eaa5dce5922af2c7250589fb1c9f8a8ab2ceaf31503f4cc19bbf13c2eb30e94da206356b91c67971fb42466442ca6678624b096ddfad1e5549178ae62cc8c153)
set(archive_sha512 "${sha512_${system}_${arch}}")
if(NOT archive_sha512)
    message(FATAL_ERROR "Orbit C has no release archive for ${TARGET_TRIPLET}")
endif()

set(archive_name "claviron-c-${VERSION}-${system}-${arch}.tar.gz")
vcpkg_download_distfile(archive
    URLS "https://github.com/claviron/c/releases/download/v${VERSION}/${archive_name}"
    FILENAME "${archive_name}"
    SHA512 "${archive_sha512}")
vcpkg_extract_source_archive(source ARCHIVE "${archive}")

# Both CMake configurations use the same optimized native build.
set(VCPKG_BUILD_TYPE release)
set(VCPKG_POLICY_MISMATCHED_NUMBER_OF_BINARIES enabled)
if(VCPKG_LIBRARY_LINKAGE STREQUAL "static")
    set(library libclaviron_orbit.a)
else()
    set(library "libclaviron_orbit.${shared_extension}")
endif()

file(INSTALL "${source}/include/" DESTINATION "${CURRENT_PACKAGES_DIR}/include")
file(INSTALL "${source}/lib/${library}" DESTINATION "${CURRENT_PACKAGES_DIR}/lib")
file(INSTALL "${source}/lib/cmake" "${source}/lib/pkgconfig"
    DESTINATION "${CURRENT_PACKAGES_DIR}/lib")
vcpkg_cmake_config_fixup(PACKAGE_NAME OrbitC CONFIG_PATH lib/cmake/OrbitC)
vcpkg_fixup_pkgconfig()
vcpkg_install_copyright(FILE_LIST
    "${source}/share/licenses/orbit-c/LICENSE-MIT"
    "${source}/share/licenses/orbit-c/LICENSE-APACHE")
file(INSTALL "${CMAKE_CURRENT_LIST_DIR}/usage" DESTINATION "${CURRENT_PACKAGES_DIR}/share/${PORT}")
