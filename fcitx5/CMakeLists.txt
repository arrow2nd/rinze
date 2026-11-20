cmake_minimum_required(VERSION 3.16)
project(fcitx5-rinze VERSION 0.1.0 LANGUAGES C CXX)

find_package(ECM REQUIRED)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH} ${CMAKE_MODULE_PATH})
find_package(Fcitx5Core REQUIRED)
include("${FCITX_INSTALL_CMAKECONFIG_DIR}/Fcitx5Utils/Fcitx5CompilerSettings.cmake")
include(GNUInstallDirs)

# crates/rinze-client を staticlib でビルドしたもの (librinze_client.a) と、cbindgen で作ったヘッダ
set(RINZE_CLIENT_LIB "" CACHE FILEPATH "librinze_client.a")
set(RINZE_CLIENT_INCLUDE "" CACHE PATH "rinze_client.h のディレクトリ")

add_library(rinze_client STATIC IMPORTED)
set_target_properties(rinze_client PROPERTIES IMPORTED_LOCATION "${RINZE_CLIENT_LIB}")

add_library(rinze MODULE src/rinze.cpp)
target_include_directories(rinze PRIVATE "${RINZE_CLIENT_INCLUDE}")
target_link_libraries(rinze PRIVATE Fcitx5::Core rinze_client)
# Rust の std のシンボルを外に出さない。rinze-client のスレッドは fcitx5 の終了まで残りうるので、dlclose で外さない
target_link_options(rinze PRIVATE
  "-Wl,--version-script=${CMAKE_CURRENT_SOURCE_DIR}/src/rinze.map"
  "-Wl,-z,nodelete")
set_target_properties(rinze PROPERTIES
  OUTPUT_NAME librinze PREFIX ""
  LINK_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/src/rinze.map"
  LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/addon-lib")
install(TARGETS rinze DESTINATION "${CMAKE_INSTALL_LIBDIR}/fcitx5")

configure_file(src/rinze-addon.conf.in "${CMAKE_BINARY_DIR}/data/addon/rinze.conf" COPYONLY)
configure_file(src/rinze.conf.in "${CMAKE_BINARY_DIR}/data/inputmethod/rinze.conf" COPYONLY)
install(FILES "${CMAKE_BINARY_DIR}/data/addon/rinze.conf" DESTINATION "${CMAKE_INSTALL_DATADIR}/fcitx5/addon")
install(FILES "${CMAKE_BINARY_DIR}/data/inputmethod/rinze.conf" DESTINATION "${CMAKE_INSTALL_DATADIR}/fcitx5/inputmethod")

option(ENABLE_TEST "testfrontend で打鍵を送るテスト" ON)
if(ENABLE_TEST)
  enable_testing()
  find_package(Fcitx5ModuleTestFrontend REQUIRED)
  add_executable(testrinze test/testrinze.cpp)
  target_compile_definitions(testrinze PRIVATE TESTING_BINARY_DIR="${CMAKE_BINARY_DIR}")
  target_link_libraries(testrinze Fcitx5::Core Fcitx5::Module::TestFrontend)
  add_dependencies(testrinze rinze)
  add_test(NAME testrinze COMMAND sh "${CMAKE_CURRENT_SOURCE_DIR}/test/run.sh" "$<TARGET_FILE:testrinze>")
  add_test(NAME exports COMMAND "${CMAKE_COMMAND}" -DNM=${CMAKE_NM} -DLIBRARY=$<TARGET_FILE:rinze>
    -P "${CMAKE_CURRENT_SOURCE_DIR}/test/exports.cmake")
endif()
