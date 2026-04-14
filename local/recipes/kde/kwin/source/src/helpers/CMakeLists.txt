# killer helper depends on private Qt QPA headers unavailable in the reduced Redox Qt build.
# Skip it for now to keep the compositor build moving.
# add_subdirectory(killer)
add_subdirectory(wayland_wrapper)
