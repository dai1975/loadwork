# `$0 run <program> [args]...`

以下のような前処理/後処理を伴ってコマンドの実行を行う。
- ドキュメントDB からワークフローの状態を取得して実行判定、実行後は結果を保存
- オブジェクトストレージから必要なファイルのダウンロード/生成したファイルのアップロード


## 環境変数
全て `LW_` のプリフィクスが付く。

 * ワークロード設定
   - LW_TARGET_ID
     処理対象のユニーク識別子。
   - LW_WORK_NAME
     実行するワークロードの名前。`[0-9a-zA-Z]+`
   - LW_WORK_VERSION
     実行するワークロードのバージョン。`[0-9a-zA-Z_]+`
   - LW_DEPENDS_<workname>
     変数名の workname 部分には依存するワークロードの名前。
     値はセミコロンで区切ったアーティファクトのリスト。

 * ディレクトリ
   - LW_INDIR
     ソフトウェアに渡すファイルを置くディレクトリ。
     あればそのまま使う。無ければ作る。
   - LW_OUTDIR
     ソフトウェアの出力ファイルを置くディレクトリ。
     あればそのまま使う。無ければ作る。

 * MongoDB
   - LW_MONGODB_URI, LW_MONGODB_DATABASE, LW_MONGODB_COLLECTION
     以上で定まるコレクションを使う。

 * S3
   - LW_S3_ACCESS_KEY, LW_S3_SECRET_KEY, LW_S3_BUCKET, LW_S3_REGION, LW_S3_ENDPOINT, LW_S3_PATH_STYLE
     以上で定まる bucket を使う。
     - LW_S3_ACCESS_KEY
     - LW_S3_SECRET_KEY
     - LW_S3_BUCKET
     - LW_S3_REGION
       省略可。
     - LW_S3_ENDPOINT
       省略可。
     - LW_S3_PATH_STYLE
       "true" または "false". 省略可。省略時は "true" となる。
     bucket の作成は行わない。devops プロセスに於いて実施されることを想定する。

## 処理

前処理:
  1. `${LW_INDIIR}/artifacts` が無かったら作る。
  2. `${LW_OUTDIIR}/artifacts` が無かったら作る。
  3. MongoDB から、キーが `{ "id": "${LW_TARGET_ID}" }` のオブジェクトを取得し、その内容を `${LW_INDIR}/workflow.json` というファイルに書き込む。
     オブジェクトが存在しない場合は、ファイルの内容を `{ "id": "${LW_TARGET_ID}" }` とする(MongoDB には書き込まれない)。
  4. 3 の JSON から、依存ワークロードの完了を確認する。未完了なら終了する。
  5. 依存ワークのアーティファクトを S3 Bucket からダウンロードし、`${LW_INDIR}/artifacts/<work>/` にダウンロードする。
     ダウンロードできなかったら終了する。

実行:
  1. 指定実行ファイル(program)を子プロセスで実行する。
     引数は executor に渡されたものがそのまま渡される。
     環境変数は LW_TARGET_ID, LW_INDIR, LW_OUTDIR のみ渡す。
  2. 実行プログラムは、LW_INDIR, LW_OUTDIR から workflow.json やアーティファクトを適宜利用し、自身の処理を終える。
     LW_OUTDIR/metadata.json を出力した場合、その内容は後処理において `works[].metadata` に保存される。
     `name=${LW_WORKNAME} が付加され、またこのキーのオブジェクトが既にあったら上書きとなる。
     TODO: metadata.json が良い。さらに、トップレベルの .metadata.<workname> にも保存しとくと便利そうだ。
  3. 終了ステータスが 0 以外またはシグナルによって終了した場合は、executor もエラーで終わる。後処理は実行しない。

後処理:
  1. `${LW_OUTDIR}/artifacts/` 直下にある通常ファイルの内容を S3 Bucket にアップロードする。
  2. `${LW_OUTDIR}/metadata.json` があれば、その内容を読み込み、次に保存するオブジェクトの metadata プロパティの値として保存する。
  3. MongoDB のキー `{ "id":"${LW_TARGET_ID}"}` オブジェクトの、works.<work_name> に実行結果を書き込む。
