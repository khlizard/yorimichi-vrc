# スクリーンショットのメタデータ

VRChat などの画像にどんなメタデータが入っているか、このツールが JPG に変換するときにそれをどう扱うかの記録。
VRChat と cluster については、2026-11-20 に実際のファイルを調べた結果に基づく。

## VRChat の PNG

VRChat は、撮影した PNG に XMP を 1 つだけ入れる。

- 置き場所: `iTXt` チャンク。キーワードは `XML:com.adobe.xmp`。圧縮なし、言語タグは空
- 位置: `IHDR` の直後、画像データ（`IDAT`）より前（チャンクの並びは `IHDR iTXt IDAT IEND`）
- `tEXt` / `zTXt` / `eXIf` / `tIME` / ICC プロファイルは無い
- 撮影日時はファイル名にも入っている（`VRChat_2026-08-14_00-10-36.648_3840x2160.png`。ミリ秒まで）

VRChat が書き出したままの XMP（ID と名前は伏せてある）:

```xml
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
    <rdf:Description>
      <xmp:CreatorTool>VRChat</xmp:CreatorTool>
      <xmp:Author>（撮影者の表示名）</xmp:Author>
      <xmp:CreateDate>2026-08-14T00:10:36.9603210+09:00</xmp:CreateDate>
      <xmp:ModifyDate>2026-08-14T00:10:36.9603210+09:00</xmp:ModifyDate>
    </rdf:Description>
    <rdf:Description xmlns:tiff="http://ns.adobe.com/tiff/1.0/">
      <tiff:DateTime>2026-08-14T00:10:36.9603210+09:00</tiff:DateTime>
    </rdf:Description>
    <rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/">
      <dc:title>
        <rdf:Alt>
          <rdf:li xml:lang="x-default"></rdf:li>
        </rdf:Alt>
      </dc:title>
    </rdf:Description>
    <rdf:Description xmlns:vrc="http://ns.vrchat.com/vrc/1.0/">
      <vrc:WorldID>wrld_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx</vrc:WorldID>
      <vrc:WorldDisplayName>（ワールド名）</vrc:WorldDisplayName>
      <vrc:AuthorID>usr_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx</vrc:AuthorID>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
```

| 項目 | 中身 |
| --- | --- |
| `xmp:CreatorTool` | 撮影したアプリ。常に `VRChat` |
| `xmp:Author` | 撮影者の表示名 |
| `xmp:CreateDate` / `xmp:ModifyDate` / `tiff:DateTime` | 撮影日時。3 つとも同じ値。ISO 8601 で、秒の小数 7 桁と UTC からの時差付き |
| `dc:title` | 空 |
| `vrc:WorldID` | ワールドの ID（`wrld_` + UUID） |
| `vrc:WorldDisplayName` | ワールド名。`!` が似た別の文字（U+01C3）になっていることがある |
| `vrc:AuthorID` | 撮影者のユーザー ID（`usr_` + UUID） |

- `rdf:Description` に `rdf:about` が無い。名前空間ごとに `rdf:Description` を分けている
- `xpacket`（XMP の前後に付く処理命令）は付かない

### Photoshop などで保存し直した PNG

調べた PNG のうち一部は Photoshop で保存し直されていて、XMP の形が変わっていた。

- 値が要素ではなく属性で書かれ、`xpacket` で囲まれる
- `xmp:Author` が `dc:creator` に、`tiff:DateTime` が無くなる
- `xmp:ModifyDate` が保存した日時に変わり、`xmp:MetadataDate`・`dc:format`・保存の履歴（`xmpMM:*`）が増える
- `xmp:CreateDate` と `vrc:*` は残る

### VRCX が付ける情報

VRCX は画像データより後ろに `Description` というテキストチャンクで JSON を書く。
これはコード（`src-tauri/src/metadata.rs`）を書いたときの前提で、今回調べたファイルには入っていなかった。

## cluster の PNG（Web から ZIP でダウンロードしたもの）

メタデータは何も入っていない。

- チャンクは `IHDR IDAT IEND` だけ。テキスト・EXIF・ICC プロファイル・`tIME` は無い
- 1920×1080 の RGB（透過なし）
- ファイル名は UUID（`00f2529c-d373-45ed-9aa3-8361e721c4db.png`）で、日時が入っていない
- 撮影日時の手がかりは更新日時だけ。ZIP に入っていた時刻が展開時に付く
  - ZIP の時刻は 2 秒単位なので、秒は必ず偶数になる
  - 時刻は日本時間（実際に遊んでいた時間帯と一致した）
  - 展開ツールによっては展開した時刻に置き換わるので、その場合は日時が分からなくなる

## このツールが JPG に変換するときの扱い

設定の「メタデータを引き継ぐ」がオンのときだけ、次のように JPG に入れる。オフなら何も入れない。

| 元の画像にあるもの | JPG での置き場所 |
| --- | --- |
| XMP | APP1（`http://ns.adobe.com/xap/1.0/` で始まるセグメント）に、元の XMP をそのまま入れる |
| EXIF | APP1（`Exif` で始まるセグメント） |
| ICC プロファイル | APP2 |
| PNG の `Description` | EXIF の UserComment（`UNICODE` + UTF-16） |

64KB を超える XMP / EXIF は入れない。透過画像は変換せずに移すので、メタデータも元のまま残る。

### cluster の画像に入れる XMP

名前が UUID の画像（cluster とみなす）を JPG に変換するとき、元の画像に XMP が無ければ、VRChat と同じ形の XMP を作って入れる。
入れるのは撮影したアプリと撮影日時だけ。

```xml
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
    <rdf:Description>
      <xmp:CreatorTool>cluster</xmp:CreatorTool>
      <xmp:CreateDate>2026-11-19T23:09:58+09:00</xmp:CreateDate>
      <xmp:ModifyDate>2026-11-19T23:09:58+09:00</xmp:ModifyDate>
    </rdf:Description>
    <rdf:Description xmlns:tiff="http://ns.adobe.com/tiff/1.0/">
      <tiff:DateTime>2026-11-19T23:09:58+09:00</tiff:DateTime>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
```

- 日時は、整理に使う撮影日時（cluster では更新日時）。秒までで、PC のタイムゾーンの時差を付ける
- 撮影者やワールドは分からないので、`xmp:Author`・`vrc:*` は入れない
- 透過している cluster の画像は変換しないので、XMP も入らない
