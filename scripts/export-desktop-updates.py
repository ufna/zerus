#!/usr/bin/env python3
"""Export current main with only the additive desktop-updates delta; no checkout changes."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import desktop_update_surface

ROOT=Path(__file__).resolve().parents[1]

def replace_once(path,old,new):
    text=path.read_text()
    if text.count(old)!=1:raise ValueError('Desktop integration anchor changed: '+str(path))
    path.write_text(text.replace(old,new))

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--main',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    if args.output.exists():raise ValueError('Integration export already exists')
    commit=subprocess.check_output(['git','-C',str(args.main),'rev-parse','HEAD'],text=True).strip()
    args.output.mkdir(parents=True)
    with tempfile.TemporaryDirectory() as staging:
        bundle=Path(staging)/'main.tar';subprocess.run(['git','-C',str(args.main),'archive','--format=tar','--output',str(bundle),'HEAD'],check=True)
        with tarfile.open(bundle) as archive:archive.extractall(args.output,filter='data')
    for path in ['tray/src/UpdateController.cpp','tray/src/UpdateController.h','tray/src/UpdatesWidget.h','tray/tests/test_updates.cpp','tray/src/RelaySettings.h','tray/src/RelayConfiguration.h','tray/tests/test_relaysettings.cpp']:
        shutil.copyfile(ROOT/path,args.output/path)
    cmake=args.output/'tray/CMakeLists.txt'
    replace_once(cmake,'add_executable(hgs-tray\n','add_library(hgs-updates STATIC src/UpdateController.cpp src/UpdateController.h)\ntarget_include_directories(hgs-updates PUBLIC src)\ntarget_link_libraries(hgs-updates PUBLIC Qt6::Widgets Qt6::Network)\n\nadd_executable(hgs-tray\n')
    replace_once(cmake,'include(CTest)','target_link_libraries(hgs-tray PRIVATE hgs-updates)\n\ninclude(CTest)')
    settings=args.output/'tray/src/SettingsPage.h'
    replace_once(settings,'#include "RecoverySettings.h"','#include "RecoverySettings.h"\n#include "UpdatesWidget.h"')
    replace_once(settings,'tr("Processes"),tr("About")','tr("Processes"),tr("Updates"),tr("About")')
    replace_once(settings,'        auto *about=makePage','        auto *updates=makePage(tr("Updates"));updates->addWidget(new UpdatesWidget);updates->addStretch();\n        auto *about=makePage')
    replace_once(settings,'#include "UpdatesWidget.h"','#include "UpdatesWidget.h"\n#include "RelaySettings.h"')
    replace_once(settings,'tr("Updates"),tr("About")','tr("Updates"),tr("Mobile connection"),tr("About")')
    replace_once(settings,'        auto *about=makePage','        auto *mobile=new RelaySettings::Panel;mobile->setMaximumWidth(850);addPage(mobile);\n        auto *about=makePage')
    main=args.output/'tray/src/main.cpp'
    replace_once(main,'#include "AppConfig.h"','#include "AppConfig.h"\n#include "UpdateController.h"')
    replace_once(main,'    if (cfg.showSessions) agent.showSessions();\n    return app.exec();','    UpdateController::instance()->checkOnStart();\n    if (cfg.showSessions) agent.showSessions();\n    return app.exec();')
    tests=args.output/'tray/tests/CMakeLists.txt'
    with tests.open('a') as output:output.write('\nadd_executable(test_updates test_updates.cpp)\ntarget_link_libraries(test_updates PRIVATE hgs-updates Qt6::Test)\nadd_test(NAME updates COMMAND test_updates)\nset_tests_properties(updates PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")\ntarget_link_libraries(test_sessionswindow PRIVATE hgs-updates)\n')
    desktop_update_surface.apply(args.output)
    with tests.open('a') as output:output.write('target_link_libraries(test_traymenu PRIVATE hgs-updates)\n')
    with tests.open('a') as output:output.write('\nadd_executable(test_relaysettings test_relaysettings.cpp)\ntarget_include_directories(test_relaysettings PRIVATE ../src)\ntarget_link_libraries(test_relaysettings PRIVATE Qt6::Test Qt6::Widgets)\nadd_test(NAME relaysettings COMMAND test_relaysettings)\nset_tests_properties(relaysettings PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")\n')
    (args.output/'desktop-update-base.txt').write_text(commit+'\n')
    print('Exported current-main desktop '+commit+' with additive update UI')

if __name__=='__main__':main()
