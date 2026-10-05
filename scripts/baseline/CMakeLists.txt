# The C++ CoolProp baseline (VERIFICATION.md §12, PLAN.md M1.15): CoolProp v8.0.0 Release, no -march, like the wheels,
# linked as its static library target. COOLPROP_SRC is a throwaway checkout (scripts/baseline/build.sh), never
# reference/CoolProp: configure writes generated headers and data into the source tree (CMakeLists.txt:557-559).
cmake_minimum_required(VERSION 3.14)
project(coolprop_baseline CXX)

set(COOLPROP_SRC "" CACHE PATH "A throwaway checkout of CoolProp at ae81610e")
if(NOT EXISTS "${COOLPROP_SRC}/CMakeLists.txt")
  message(FATAL_ERROR "COOLPROP_SRC must be a CoolProp checkout, got '${COOLPROP_SRC}'")
endif()

add_subdirectory(${COOLPROP_SRC} coolprop EXCLUDE_FROM_ALL)

add_executable(coolprop_baseline coolprop_baseline.cpp)
target_compile_features(coolprop_baseline PRIVATE cxx_std_17)
target_link_libraries(coolprop_baseline PRIVATE ${COOLPROP_LIBRARY_NAME} ${CMAKE_DL_LIBS})
