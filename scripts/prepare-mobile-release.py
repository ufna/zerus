#!/usr/bin/env python3
"""Seal an already-built APK and exact source archive; never publish or build."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from update_feed import HEX40, HEX64, file_hash, require, safe_name


def command(argv):
    with tempfile.TemporaryFile() as output:
        try:
            result=subprocess.run(argv,stdin=subprocess.DEVNULL,stdout=output,stderr=subprocess.DEVNULL,timeout=30,check=False)
        except (OSError,subprocess.TimeoutExpired):raise ValueError('APK verification tool unavailable') from None
        require(result.returncode==0,'APK verification tool rejected artifact')
        require(output.tell()<=256*1024,'APK metadata output exceeds bounds')
        output.seek(0);return output.read().decode('utf-8')


def apk_metadata(apk,aapt2,apksigner,run=command):
    require(apk.is_file() and not apk.is_symlink() and 0<apk.stat().st_size<=128*1024**2,'Invalid APK file')
    badging=run([str(aapt2),'dump','badging',str(apk.resolve())])
    lines=badging.splitlines();packages=[line for line in lines if line.startswith('package: ')]
    sdk=[line for line in lines if line.startswith(('sdkVersion:','minSdkVersion:'))]
    require(not any(line.startswith('application-debuggable') for line in lines) and not re.search(r"\bsplit='",badging),'Distributable APK must be non-debuggable and universal')
    require(len(packages)==1 and len(sdk)==1,'Missing/unbounded APK manifest identity')
    package=re.search(r"\bname='([^']+)'",packages[0]);code=re.search(r"\bversionCode='([0-9]+)'",packages[0]);version=re.search(r"\bversionName='([^']+)'",packages[0]);minimum=re.fullmatch(r"(?:sdkVersion|minSdkVersion):'([0-9]+)'",sdk[0])
    require(package and package[1]=='app.zerus.mobile' and code and version and minimum,'APK manifest identity mismatch')
    require(0<int(code[1])<=2_100_000_000 and 26<=int(minimum[1])<=100 and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.+_-]{0,79}',version[1]),'Invalid Android version/platform')
    signatures=run([str(apksigner),'verify','--verbose','--print-certs',str(apk.resolve())])
    certificates=re.findall(r'^Signer #\d+ certificate SHA-256 digest: ([a-fA-F0-9]{64})$',signatures,re.MULTILINE)
    require(len(certificates)==1,'APK must have one verified signer')
    return {'package_name':package[1],'version_code':int(code[1]),'version_name':version[1],'min_sdk':int(minimum[1]),'signing_cert_sha256':certificates[0].lower()}


def seal(apk,source,output,commit,channel,dirty,aapt2,apksigner,run=command):
    require(HEX40.fullmatch(commit or '') and channel in {'dev','stable','nightly'},'Invalid source/channel')
    require(not dirty or channel=='dev','Dirty snapshots are development-only')
    require(source.is_file() and not source.is_symlink() and source.name.endswith('.tar.gz') and 0<source.stat().st_size<=512*1024**2,'Exact source archive required')
    require(apk.name.endswith('.apk'),'APK filename required');safe_name(apk.name);safe_name(source.name)
    require(apk.name!=source.name and apk.name not in {'release-info.json','SHA256SUMS'} and source.name not in {'release-info.json','SHA256SUMS'},'Artifact filename collision')
    require(not output.exists(),'Release output already exists')
    identity=apk_metadata(apk,aapt2,apksigner,run)
    info={'schema':1,'platform':'android','channel':channel,'publication':'candidate','source_commit':commit,'source_dirty':bool(dirty),**identity,'artifacts':[]}
    # Hash and verify frozen copies, not mutable build outputs. Candidate directory
    # appears only after all metadata/checksums are ready.
    output.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.mobile-release-',dir=output.parent) as staging:
        root=Path(staging)
        for kind,path in [('apk',apk),('source',source)]:
            destination=root/path.name;shutil.copyfile(path,destination)
            require(file_hash(destination)==file_hash(path),'Artifact changed during sealing')
            info['artifacts'].append({'kind':kind,'name':path.name,'sha256':file_hash(destination),'size':destination.stat().st_size})
        require(apk_metadata(root/apk.name,aapt2,apksigner,run)==identity,'APK changed during sealing')
        if dirty:info['source_snapshot']={k:v for k,v in info['artifacts'][1].items() if k!='kind'}
        (root/'release-info.json').write_text(json.dumps(info,indent=2,sort_keys=True)+'\n')
        (root/'SHA256SUMS').write_text(''.join(file_hash(path)+'  '+path.name+'\n' for path in sorted(root.iterdir())))
        root.rename(output)
    return info


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apk',required=True,type=Path);parser.add_argument('--source-snapshot',required=True,type=Path)
    parser.add_argument('--source-commit',required=True);parser.add_argument('--source-dirty',action='store_true')
    parser.add_argument('--channel',choices=['dev','stable','nightly'],default='dev')
    parser.add_argument('--aapt2',required=True,type=Path);parser.add_argument('--apksigner',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    try:
        info=seal(args.apk,args.source_snapshot,args.output,args.source_commit,args.channel,args.source_dirty,args.aapt2,args.apksigner)
        print(json.dumps({'versionName':info['version_name'],'versionCode':info['version_code'],'channel':info['channel'],'sourceDirty':info['source_dirty'],'artifacts':info['artifacts']},sort_keys=True));return 0
    except (ValueError,OSError,UnicodeError):
        print('APK/source sealing failed; no candidate published',file=sys.stderr);return 1


if __name__=='__main__':sys.exit(main())
