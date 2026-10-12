# EPUB Qt prototip

Qt `QTextDocument` paged-layout pomoćni renderer koji koristi glavna Rust/Slint
aplikacija na Linux desktopu.

Build i pokretanje iz korijena projekta:

```sh
cmake -S prototype/epub_qt -B prototype/epub_qt/build -G Ninja \
  -DCMAKE_BUILD_TYPE=Release
cmake --build prototype/epub_qt/build

prototype/epub_qt/build/citac-epub-qt-prototype \
  /putanja/do/raspakovanog-epub-a /putanja/do/spine.txt \
  prototype/epub_qt/output 3 10
```

Argumenti su: pripremljeni EPUB direktorijum, tekstualna spine lista, izlazni
direktorijum, maksimalan broj PNG stranica, osnovna veličina fonta i opciona prva stranica
(brojanje počinje od 1). Dokument se postavlja na stranicu 600×800.
Izlazni `QImage::Format_RGBA8888` ima četiri bajta po pikselu i može se kopirati u
Slint `SharedPixelBuffer<Rgba8Pixel>` pri kasnijoj integraciji.

Rust dio aplikacije provjerava putanje, limite i strukturu arhive, raspakuje EPUB
i Qt-u predaje samo pripremljeni korijenski direktorijum i spine redoslijed.
Renderer zato više ne koristi privatni Qt `QZipReader` API.

## Rezultat provjere

Prototip prati bitne korake stvarnog Okular EPUB generatora: koristi jedan
`QTextDocument` za cijeli spine, stranicu 600×800, sadržaj širine najviše 560 i
visine 760, prilagođava EPUB CSS koji Qt ne razumije i svaki spine dio započinje
na novoj stranici.

Na `Test_sa_vise_slika.epub`, sa fontom 10 pt:

- naslovnica odgovara izvornom rasporedu i staje na jednu stranicu;
- slike i pasusi se prelamaju na stvarnim Qt stranicama umjesto prostog rezanja
  kontinuiranog dokumenta;
- rezultat je 123 stranice, jednu manje od 124 prikazane u korisnikovom Okularu;
- bez rasterizovanja vršna memorija je približno 48 MiB i obrada traje oko 0,29 s;
- izvršni fajl ima oko 118 KiB, ali dinamički zavisi od Qt6 Core i Gui biblioteka
  koje same zauzimaju približno 17 MiB, ne računajući njihove zavisnosti;
- početni direktni Qt pristup dostizao je oko 442 MiB kada bi layout učitao veliki
  broj slikovnih resursa;
- ograničeni pristup sada upisuje stvarne dimenzije u HTML, za layout koristi
  prazne resurse i za svaku traženu stranicu pravi svjež render dokument koji
  dekodira samo njene slike;
- prvih pet stranica i direktno otvaranje stranice 120 ostaju na približno
  64 MiB; prvih pet su piksel-po-piksel jednake prethodnom Qt rasteru;
- priprema i rasterizovanje nasumične stranice traje približno 0,5 s, dok pet
  uzastopnih stranica bez keša traje približno 1,55 s.

Qt rješenje je zato znatno bliže Okularu od Blitz prototipa, ispravno rješava
prelamanje i sada ima ograničenu memoriju. Qt za Android ipak nije dio trenutnog
projekta, a paralelno uključivanje Qt i Slint GUI runtimea povećalo bi paket i
složenost lifecyclea. Prije produkcijske integracije treba odlučiti da li će Qt
biti opcionalni desktop backend ili opravdati cijenu zasebnog Qt Android runtimea.
