# SPDX-FileCopyrightText: KDE Contributors
# SPDX-License-Identifier: BSD-2-Clause

add_subdirectory(core)

# add_subdirectory(qtquick) # disabled for Redox core-only build
# add_subdirectory(tools) # disabled for Redox core-only build

# add_subdirectory(widgets) # disabled for Redox core-only build
ecm_qt_install_logging_categories(
    EXPORT KNEWSTUFF
    FILE knewstuff.categories
    DESTINATION ${KDE_INSTALL_LOGGINGCATEGORIESDIR}
)
