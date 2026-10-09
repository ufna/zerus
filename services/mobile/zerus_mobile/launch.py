"""Allowlisted native session creation and privacy-minimal local catalogs."""
import re
from pathlib import PurePosixPath
from .context import bounded

OPERATIONS = frozenset({'catalog','dirs','launch'})
AGENTS = frozenset({'codex','claude','kimi','dsh'})


def validate(operation, payload, request_id):
    fields = {'catalog': {'request_id'}, 'dirs': {'request_id','path'},
              'launch': {'request_id','agent','directory','tag'}}[operation]
    if not isinstance(payload, dict) or not fields <= set(payload) or set(payload)-fields-({'account_id'} if operation=='launch' else set()) or payload.get('request_id')!=request_id:
        raise ValueError('invalid scoped catalog/launch fields')
    if operation=='dirs':
        path=bounded(payload['path'],4096)
        if path!='~' and not PurePosixPath(path).is_absolute(): raise ValueError('directory must be absolute or home')
    if operation=='launch':
        if not isinstance(payload['agent'],str) or payload['agent'] not in AGENTS: raise ValueError('unsupported native provider')
        directory=bounded(payload['directory'],4096)
        if not PurePosixPath(directory).is_absolute(): raise ValueError('launch directory must be absolute')
        tag=bounded(payload['tag'],120)
        if tag.startswith('-') or any(c.isspace() or c in '/:.' for c in tag): raise ValueError('invalid native session tag')
        if 'account_id' in payload:
            value=bounded(payload['account_id'],80)
            if not re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_-]*',value): raise ValueError('invalid native account identifier')


def catalog(raw):
    if not isinstance(raw,dict) or not isinstance(raw.get('profiles'),list) or len(raw['profiles'])>1000:
        raise ValueError('native account catalog is invalid or too large')
    accounts=[];agents=set()
    for row in raw['profiles']:
        if not isinstance(row,dict) or row.get('provider') not in AGENTS: continue
        account={key:bounded(row.get(key),160 if key=='label' else 80) for key in ('id','provider','label')}
        accounts.append(account)
        if row.get('installed') is True: agents.add(row['provider'])
    return {'accounts':accounts,'agents':sorted(agents)}
