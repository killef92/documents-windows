# Triplet vcpkg pour libheif + libde265 (photos HEIC) : DLL 64 bits, version optimisée seulement,
# avec la bibliothèque C/C++ de Microsoft intégrée (rien d'autre à installer sur le PC).
set(VCPKG_TARGET_ARCHITECTURE x64)
set(VCPKG_CRT_LINKAGE static)
set(VCPKG_LIBRARY_LINKAGE dynamic)
set(VCPKG_BUILD_TYPE release)
