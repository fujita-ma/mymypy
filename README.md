# mymypy

gitと連動して、リビジョン間で変更されたpythonソースコードのmypy出力の差分をとるやつ

## install(仮)

```
cargo build --release
export PATH=$PATH:/path/to/mymypy/target/release
```

## つかいかた

現在のワークツリーとHEADの差分を表示

```
mymypy
```

現在のワークツリーと指定したリビジョンの差分を表示

```
mymypy HEAD~
```

指定した２つのリビジョンの差分を表示

```
mymypy develop my-branch
```
