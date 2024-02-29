python -m venv "$env:APPDATA\Divvun Runtime\tts-venv"
"$env:APPDATA\Divvun Runtime\tts-venv\Scripts\Activate.ps1"

function New-TemporaryDirectory {
    $parent = [System.IO.Path]::GetTempPath()
    [string] $name = [System.Guid]::NewGuid()
    New-Item -ItemType Directory -Path (Join-Path $parent $name) | Out-Null
    (Join-Path $parent $name)
}

$tmp = New-TemporaryDirectory

pip install Cython wheel setuptools poetry
pip install torch==2.1 torchvision==0.16 cdifflib
pip install --no-use-pep517 'nemo_toolkit[tts] @ git+https://github.com/divvun/NeMo'
pip install psutil==5.9.3

$cur=pwd
cd $tmp
git clone git@github.com:divvun/divvun-speech-py
cd divvun-speech-py
poetry build
pip install dist\divvun_speech-0.1.0-py3-none-any.whl --force-reinstall
cd $pwd

pip uninstall -y Cython wheel poetry