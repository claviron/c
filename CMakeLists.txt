cmake_minimum_required(VERSION 3.20)
project(OrbitC VERSION 0.1.2 LANGUAGES C)
include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

set(ORBIT_NATIVE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/target/release" CACHE PATH "Cargo-built native library directory")
set(ORBIT_STATIC_LINK_FLAGS "" CACHE STRING "Additional native link flags reported by rustc for static consumers")
find_library(ORBIT_NATIVE_LIBRARY NAMES claviron_orbit PATHS "${ORBIT_NATIVE_DIR}" NO_DEFAULT_PATH REQUIRED)
add_library(OrbitC::orbit UNKNOWN IMPORTED GLOBAL)
set_target_properties(OrbitC::orbit PROPERTIES IMPORTED_LOCATION "${ORBIT_NATIVE_LIBRARY}"
  INTERFACE_INCLUDE_DIRECTORIES "${CMAKE_CURRENT_SOURCE_DIR}/include")
find_package(Threads REQUIRED)
foreach(example echo echo-low-level abi-check runtime-check dispatcher-check)
  add_executable(orbit-${example} examples/${example}.c)
  target_compile_features(orbit-${example} PRIVATE c_std_11)
  target_link_libraries(orbit-${example} PRIVATE OrbitC::orbit Threads::Threads)
endforeach()

install(FILES include/orbit.h DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}/claviron")
file(GLOB native_artifacts "${ORBIT_NATIVE_DIR}/libclaviron_orbit.so" "${ORBIT_NATIVE_DIR}/libclaviron_orbit.dylib" "${ORBIT_NATIVE_DIR}/libclaviron_orbit.a")
install(FILES ${native_artifacts} DESTINATION "${CMAKE_INSTALL_LIBDIR}")
install(FILES LICENSE-MIT LICENSE-APACHE DESTINATION "${CMAKE_INSTALL_DATADIR}/licenses/orbit-c")
configure_package_config_file(cmake/OrbitCConfig.cmake.in "${CMAKE_CURRENT_BINARY_DIR}/OrbitCConfig.cmake"
  INSTALL_DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/OrbitC")
write_basic_package_version_file("${CMAKE_CURRENT_BINARY_DIR}/OrbitCConfigVersion.cmake"
  VERSION ${PROJECT_VERSION} COMPATIBILITY SameMajorVersion)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/OrbitCConfig.cmake" "${CMAKE_CURRENT_BINARY_DIR}/OrbitCConfigVersion.cmake"
  DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/OrbitC")
configure_file(cmake/orbit-c.pc.in "${CMAKE_CURRENT_BINARY_DIR}/orbit-c.pc" @ONLY)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/orbit-c.pc" DESTINATION "${CMAKE_INSTALL_LIBDIR}/pkgconfig")
