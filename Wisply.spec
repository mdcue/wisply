# -*- mode: python ; coding: utf-8 -*-
# Build with: pyinstaller --noconfirm Wisply.spec
# Run from the project root (the folder containing main.py), venv active.
# Output: dist\Wisply\Wisply.exe (folder build, not single-file)

a = Analysis(
    ['main.py'],
    pathex=[],
    binaries=[],
    datas=[
        ('ui/hub_web', 'ui/hub_web'),
        ('ui/assets', 'ui/assets'),
    ],
    hiddenimports=[],
    hookspath=[],
    hooksconfig=[],
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='Wisply',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,
    icon='ui/assets/icon.ico',
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    name='Wisply',
)