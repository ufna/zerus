"""Bounded native worktree evidence; no Git commands run on the relay."""
from pathlib import PurePosixPath
from .context import bounded

OPERATIONS = frozenset({'worktrees', 'worktree_create'})


def absolute(value):
    value = bounded(value, 4096)
    if not PurePosixPath(value).is_absolute():
        raise ValueError('worktree path must be absolute')
    return value


def validate(operation, payload, request_id):
    fields = {'request_id', 'path'} if operation == 'worktrees' else {
        'request_id', 'path', 'common_dir', 'destination', 'branch', 'base'}
    if not isinstance(payload, dict) or set(payload) != fields or payload.get('request_id') != request_id:
        raise ValueError('invalid scoped worktree fields')
    absolute(payload['path'])
    if operation == 'worktree_create':
        absolute(payload['common_dir']); absolute(payload['destination'])
        for key in ('branch', 'base'):
            value = bounded(payload[key], 256)
            if value.strip() != value or value.startswith('-'):
                raise ValueError('invalid worktree branch or revision')


def catalog(raw, requested):
    if not isinstance(raw, dict) or not isinstance(raw.get('worktrees'), list) or len(raw['worktrees']) > 512:
        raise ValueError('invalid native worktree catalog')
    state = bounded(raw.get('state'), 32)
    result = {'state': state, 'path': absolute(raw.get('path')), 'requested_path': requested,
              'stale': raw.get('stale') is True, 'partial': raw.get('partial') is True, 'worktrees': []}
    for key in ('common_dir', 'selected_root'):
        if raw.get(key) is not None: result[key] = absolute(raw[key])
    if state == 'ok' and not result.get('common_dir'): raise ValueError('worktree repository identity is missing')
    for row in raw['worktrees']:
        if not isinstance(row, dict) or row.get('kind') not in ('main', 'linked', 'bare'):
            raise ValueError('invalid native worktree entry')
        item = {'path': absolute(row.get('path')), 'kind': row['kind'], 'available': row.get('available') is True}
        for key in ('branch', 'head'):
            if row.get(key): item[key] = bounded(row[key], 256)
        for key in ('detached', 'locked', 'prunable'):
            item[key] = row.get(key) is True
        result['worktrees'].append(item)
    return result


def contains(catalog, path):
    if catalog.get('state') != 'ok' or catalog.get('stale') or not catalog.get('common_dir'): return False
    selected = PurePosixPath(path)
    return any(row.get('available') is True and row.get('kind') != 'bare' and
               selected == PurePosixPath(row['path'])
               for row in catalog.get('worktrees', []))


def created(raw, payload, request_id, destination):
    if (not isinstance(raw, dict) or raw.get('request_id') != request_id or raw.get('status') != 'created'
            or raw.get('common_dir') != payload['common_dir'] or raw.get('branch') != payload['branch']
            or raw.get('path') != destination):
        raise ValueError('native worktree creation was not confirmed by exact identity')
    return {'request_id': request_id, 'status': 'created', 'path': absolute(raw.get('path')),
            'common_dir': absolute(raw['common_dir']), 'branch': bounded(raw['branch'], 256)}
