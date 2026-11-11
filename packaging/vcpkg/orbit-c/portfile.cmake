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

set(sha512_freebsd_aarch64 34847ecc2699e252145ad576d42f64d1e018d8c5dd344d13783efffa47c8c98019c725861989f5c5951d1b85368d2b3a35fee148d92e492ec22bdda3f90af5f4)
set(sha512_freebsd_x86_64 355ca1705b904fda91ae5082fee7ac5c9c426a81bd70c1591a9f10e8967389bbaa8eb966ade3749c268078f6cee26272c78cad650035cda702c6411b4d710b31)
set(sha512_linux_aarch64 95b642ef0229e1e7eefef58a46b52b711adee98be5738e6b409181d3468befee06689d064833a5586037af75f57572b42a0753c5580f0396bc1a464c2d3282ab)
set(sha512_linux_x86_64 8238de7642914767f419093ec828a7bfcdbbaadd6b9acef4790896c9be9d2c6251c3963e50329051294d31afabff839ffae5a27df7423b327b55c1bb118fa221)
set(sha512_macos_aarch64 b5841e93d78e1309b7356031bf88bf05031c28d6299deb93be0daef3af32dcefeab68762c573602de337cf39f8f75fbe946b6dcc7df8af9c47f7f6d9c2d96a19)
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
