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
import zipfile

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


def mapping_identity(apk, mapping):
    """Verify R8 mapping against the APK embedded compiler marker."""
    require(mapping.is_file() and not mapping.is_symlink() and 0 < mapping.stat().st_size <= 128*1024**2, 'Invalid R8 mapping')
    text=mapping.read_text(encoding='utf-8')
    ids=re.findall(r'^# pg_map_id: ([a-f0-9]{7,64})$', text, re.MULTILINE)
    require(len(ids)==1, 'R8 mapping identity missing')
    markers=set()
    with zipfile.ZipFile(apk) as archive:
        for entry in archive.infolist():
            if not re.fullmatch(r'classes(?:[0-9]+)?\.dex', entry.filename):continue
            require(entry.file_size <= 128*1024**2, 'DEX exceeds bounds')
            for marker in re.finditer(rb'~~R8(\{[^\x00]{1,2048}\})', archive.read(entry)):
                try: identity=json.loads(marker[1])
                except (ValueError,UnicodeError):continue
                if identity.get('pg-map-id'):markers.add(identity['pg-map-id'])
    require(markers=={ids[0]}, 'R8 mapping does not match APK')
    return ids[0]


def retain_diagnostics(apk, mapping, output, release_info):
    """Keep exact R8 diagnostics outside the public release directory."""
    require(not output.exists(), 'Private diagnostics output already exists')
    map_id=mapping_identity(apk,mapping)
    apk_artifact=next(item for item in release_info['artifacts'] if item['kind']=='apk')
    require(file_hash(apk)==apk_artifact['sha256'], 'APK differs from sealed release')
    output.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.mobile-diagnostics-',dir=output.parent) as staging:
        root=Path(staging);root.chmod(0o700)
        destination=root/'mapping.txt';shutil.copyfile(mapping,destination);destination.chmod(0o600)
        require(file_hash(destination)==file_hash(mapping), 'Mapping changed during retention')
        require(mapping_identity(apk,destination)==map_id, 'Mapping changed after validation')
        identity={'schema':1,'source_commit':release_info['source_commit'],'source_dirty':release_info['source_dirty'],
                  'version_name':release_info['version_name'],'version_code':release_info['version_code'],
                  'signing_cert_sha256':release_info['signing_cert_sha256'],'apk_sha256':apk_artifact['sha256'],
                  'source_sha256':next(item['sha256'] for item in release_info['artifacts'] if item['kind']=='source'),
                  'mapping_sha256':file_hash(destination),'r8_map_id':map_id}
        metadata=root/'diagnostics-info.json';metadata.write_text(json.dumps(identity,indent=2,sort_keys=True)+'\n');metadata.chmod(0o600)
        root.rename(output)
    return identity


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apk',required=True,type=Path);parser.add_argument('--source-snapshot',required=True,type=Path)
    parser.add_argument('--source-commit',required=True);parser.add_argument('--source-dirty',action='store_true')
    parser.add_argument('--channel',choices=['dev','stable','nightly'],default='dev')
    parser.add_argument('--aapt2',required=True,type=Path);parser.add_argument('--apksigner',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path)
    parser.add_argument('--mapping',type=Path);parser.add_argument('--diagnostics-output',type=Path)
    args=parser.parse_args()
    try:
        require(bool(args.mapping)==bool(args.diagnostics_output), 'Mapping requires a separate private diagnostics directory')
        if args.diagnostics_output:
            require(args.diagnostics_output.resolve()!=args.output.resolve() and args.output.resolve() not in args.diagnostics_output.resolve().parents, 'Diagnostics must be outside public candidate')
            require(not args.diagnostics_output.exists(), 'Private diagnostics output already exists')
            mapping_identity(args.apk,args.mapping)
        info=seal(args.apk,args.source_snapshot,args.output,args.source_commit,args.channel,args.source_dirty,args.aapt2,args.apksigner)
        if args.mapping:retain_diagnostics(args.output/args.apk.name,args.mapping,args.diagnostics_output,info)
        print(json.dumps({'versionName':info['version_name'],'versionCode':info['version_code'],'channel':info['channel'],'sourceDirty':info['source_dirty'],'artifacts':info['artifacts']},sort_keys=True));return 0
    except (ValueError,OSError,UnicodeError,zipfile.BadZipFile):
        print('APK/source sealing failed; no candidate published',file=sys.stderr);return 1


if __name__=='__main__':sys.exit(main())
