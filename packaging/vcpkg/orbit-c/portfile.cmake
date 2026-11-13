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

set(sha512_freebsd_aarch64 b6d391a21c77255ab6ffaff475b66608af0b39a047f80cd16e7907e7037b6d1e302d2317ef32c604226b8c65d7748a788177c194ed73d46b6e7a451305a9d929)
set(sha512_freebsd_x86_64 122bee840fa9dd3ee5bf5314052b13067498d2b3d0e7c4dfde83b402ea6caaaa05027e80ddb81549c05d760180bbef771f4a9305c9a7d28fabd5f583a2fa4123)
set(sha512_linux_aarch64 08a0c6e747aa6790c033b909a7c1cf10bc258d54eb3db5ddc49936b67230be8c6300bbd8407d7e3f91814db672254c3a2508619efddb15543503eaeb46fc838d)
set(sha512_linux_x86_64 0793b2ca1542806211f359c92aaf1c06d4f968b3d77196251f5258454ab4f96840f822d95feeaa1af5776dfd629cf136c5fd9163720bf61eac757c2099330e24)
set(sha512_macos_aarch64 79e216526bdf930f0b38c7fd37fb02f6f738897fc6527d6e2976483f1d7a7e10926c51629698ba89a9d0edd3325b6fd0ad95a2024d544e96de0efb0857487eb3)
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
