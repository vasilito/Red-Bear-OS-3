find_package(Qt6Test REQUIRED)

ecm_add_test(migrateconfigtest.cpp LINK_LIBRARIES Qt::Test KF6::ConfigCore  KF6::Service KGlobalAccelD)
