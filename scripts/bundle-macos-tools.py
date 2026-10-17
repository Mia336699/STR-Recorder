#!/usr/bin/env python3
"""Bundle local FFmpeg tools with their non-system dylib closure and relocate load commands."""
from pathlib import Path
import subprocess, shutil, json, os
root=Path(__file__).resolve().parents[1]
out=root/'desktop/src-tauri/resources/bin'
out.mkdir(parents=True,exist_ok=True)
def run(*args): return subprocess.check_output(args,text=True,stderr=subprocess.STDOUT)
def deps(path):
    return [line.strip().split(' (')[0] for line in run('otool','-L',str(path)).splitlines()[1:]]
sources={}
def collect(source):
    source=Path(source).resolve(); name=source.name
    if name in sources:
        if sources[name]!=source: raise RuntimeError(f'Library basename collision: {source}')
        return
    sources[name]=source
    if (out/name).exists(): (out/name).unlink()
    shutil.copy2(source,out/name)
    (out/name).chmod((out/name).stat().st_mode | 0o200)
    for dep in deps(source):
        if dep.startswith('/opt/homebrew/'): collect(dep)
        elif dep.startswith('@'): raise RuntimeError(f'Unresolved dependency {dep} from {source}')
# Never select a previous generated copy as its own source when PATH includes it.
source_path = os.pathsep.join(p for p in os.environ.get('PATH', '').split(os.pathsep)
    if Path(p).resolve() != out.resolve())
for name in ['ffmpeg','ffprobe']:
    source = shutil.which(name, path=source_path)
    if not source: raise RuntimeError(f'Missing build dependency: {name}')
    collect(source)
for name,source in sources.items():
    dest=out/name
    subprocess.run(['codesign','--remove-signature',str(dest)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    if name.endswith('.dylib'): run('install_name_tool','-id','@loader_path/'+name,str(dest))
    for dep in deps(source):
        if dep.startswith('/opt/homebrew/'):
            run('install_name_tool','-change',dep,'@loader_path/'+Path(dep).resolve().name,str(dest))
    run('codesign','--force','--sign','-',str(dest))
for name in ['ffmpeg','ffprobe']:
    run(str(out/name),'-version')
licenses=out.parent/'licenses'
licenses.mkdir(exist_ok=True)
if (root/'LICENSE').exists():
    shutil.copy2(root/'LICENSE',licenses/'StripchatRecorder-GPL-3.0.txt')
else:
    (licenses/'StripchatRecorder-GPL-3.0.txt').write_text(run('git','-C',str(root),'show','HEAD:LICENSE'))
packages={}
for source in sources.values():
    parts=source.parts
    if 'Cellar' in parts:
        idx=parts.index('Cellar'); pkg=Path(*parts[:idx+3]); packages[pkg.name+"-"+pkg.parent.name]=pkg
for label,pkg in packages.items():
    target=licenses/label; target.mkdir(exist_ok=True)
    for item in pkg.iterdir():
        if item.is_file() and any(word in item.name.upper() for word in ['LICENSE','COPYING','NOTICE','RECEIPT','SBOM']):
            shutil.copy2(item,target/item.name)
    recipes=pkg/'.brew'
    if recipes.exists(): shutil.copytree(recipes,target/'build-recipes',dirs_exist_ok=True)
(licenses/'FFmpeg-build.txt').write_text(run(str(out/'ffmpeg'),'-version')+'\nSource: https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz\nHomebrew build recipes and dependency source URLs are included alongside the licenses.\n')
(out.parent/'tools-manifest.json').write_text(json.dumps({n:str(s) for n,s in sources.items()},indent=2)+'\n')
print(f'Bundled {len(sources)} binaries/libraries, {sum(p.stat().st_size for p in out.iterdir())//1024//1024} MiB')
