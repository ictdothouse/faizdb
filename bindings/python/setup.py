from setuptools import setup, find_packages

setup(
    name="faizdb",
    version="0.1.0",
    description="Official Python SDK for FaizDB — The Universal High-Performance Multi-Model Database Engine",
    author="Ahmad Faiz",
    author_email="faiz@ict.house",
    url="https://github.com/ictdothouse/faizdb",
    packages=find_packages(),
    classifiers=[
        "Programming Language :: Python :: 3",
        "License :: OSI Approved :: Apache Software License",
        "Operating System :: OS Independent",
        "Topic :: Database :: Database Engines/Servers",
    ],
    python_requires=">=3.8",
)
