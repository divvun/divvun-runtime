#!/bin/bash

python3.11 -m venv tts-venv
source tts-venv/bin/activate
pip install Cython wheel setuptools
pip install torch==2.1 torchvision==0.16 cdifflib
pip install --no-use-pep517 'nemo_toolkit[tts] @ git+https://github.com/divvun/NeMo'

tmp=`mktemp -d`
pushd $tmp
git clone https://github.com/divvun/divvun-speech-py
cd divvun-speech-py
poetry build
pip install dist/*.whl --force-reinstall
popd

rm -rf $tmp