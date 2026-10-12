# Čitač — faze rada

Ovaj dokument prati stvarno stanje projekta. Stavka je završena tek kada je implementirana i provjerena odgovarajućim formatiranjem, testovima ili buildom.

## Verzija

- Trenutna verzija: `0.11.2`
- Verzija se prikazuje na početnoj strani aplikacije.
- Nova funkcionalna faza povećava srednji broj verzije, a manja popravka posljednji broj.
- Verzija `1.0.0` je rezervisana za stabilno prvo izdanje.

## Faza 1 — osnova projekta i probna navigacija

Status: završeno (`0.1.0`)

- [x] Potvrđeno početno stanje projekta.
- [x] Postavljena osnovna Rust/Cargo struktura.
- [x] Dodat Slint GUI i zajednička verzija aplikacije.
- [x] Dodata navigacija: Biblioteka, Omiljene, Čitanje i Pretraga.
- [x] Dodati fiktivni folderi, knjige i stripovi.
- [x] Omogućeno otvaranje probnog foldera i privremenog čitača.
- [x] Navigacija se skriva unutar foldera i čitača.
- [x] Potvrđeni `cargo fmt`, test profil i Linux build.
- [x] Potvrđeno pokretanje prozora na virtuelnom Linux/X11 displeju.
- [x] Korisnički potvrđen izgled i navigacija.

## Faza 2 — model biblioteke i lokalni podaci

Status: završeno (`0.2.0`)

- [x] Rust model foldera i naslova, uključujući više foldera za isti naslov.
- [x] SQLite baza, foreign-key pravila i početna transakciona migracija.
- [x] Trajno čuvanje omiljenih, preciznog lokatora i procenta napretka.
- [x] Uklanjanje iz sekcije „Čitanje“ bez brisanja naslova ili sačuvane pozicije.
- [x] Ponovni progress automatski vraća naslov u sekciju „Čitanje“.
- [x] Hijerarhijska navigacija kroz probni podfolder.
- [x] Brisanje foldera uklanja članstvo, ali ne briše naslov.
- [x] Probni sadržaj se unosi samo u praznu bazu i ne duplira se.
- [x] GUI modeli se osvježavaju nakon izmjene omiljenih i sekcije „Čitanje“.
- [x] Dodato šest testova za navigaciju, članstvo, brisanje, progress, omiljene i inicijalni unos.
- [x] Potvrđeni formatiranje, testovi, Linux build i vizuelni smoke test.

## Faza 3 — uvoz i upravljanje bibliotekom

Status: završeno (`0.3.0`, ispravka `0.3.1`)

- [x] Desktop file picker za jedan ili više fajlova.
- [x] Rekurzivni uvoz foldera uz očuvanje hijerarhije podfoldera.
- [x] Uvoze se samo EPUB, CBR, CBZ i TXT fajlovi.
- [x] SHA-256 identitet zasnovan na sadržaju fajla.
- [x] Veličina i vrijeme izmjene sprečavaju ponovno hashovanje nepromijenjenog fajla.
- [x] File picker, skeniranje foldera i hashovanje rade van GUI niti.
- [x] Kreiranje i preimenovanje foldera; polje za naziv je inače sakriveno.
- [x] Režim izbora sa posebnom potvrdom uklanjanja više naslova ili foldera.
- [x] Uklanjanje naslova iz foldera briše samo članstvo.
- [x] Uklanjanje iz biblioteke i brisanje foldera nikada ne brišu izvorne fajlove.
- [x] Funkcionalna Unicode-aware pretraga po dijelu naslova ili autora.
- [x] Dodata SQLite migracija verzije 2 za jedinstvene nazive foldera unutar roditelja.
- [x] Dodata ukupno četiri nova testa za pretragu, pojedinačni uvoz, hijerarhijski uvoz i bezbjedno uklanjanje.
- [x] Svih deset testova, formatiranje, Linux build i vizuelni smoke test prolaze.
- [x] Korisnički potvrđen rad sistemskog file pickera na TumbleweedOS-u.

### Ispravka 0.3.1

- [x] Početna biblioteka je eksplicitna lokacija, nezavisna od folder članstava.
- [x] Ponovni uvoz istog sadržaja u folder ne uklanja naslov sa početne strane.
- [x] Isti naslov može istovremeno biti na početnoj strani i u više foldera.
- [x] Uklanjanje sa početne strane ne uklanja isti naslov iz foldera.
- [x] Dodata SQLite migracija verzije 3 i dva regresiona testa za višestruke lokacije.
- [x] Dugme „Izaberi“ preimenovano je u „Obriši“, a završna akcija u „Potvrdi brisanje“.

## Faza 4 — EPUB i TXT čitač

Status: završeno (`0.4.0`, proširenja `0.4.1`–`0.4.10`, dovršeno u `0.8.0`)

- [x] Otvaranje stvarno uvezenih TXT i EPUB fajlova iz sačuvane putanje.
- [x] Učitavanje i parsiranje u radnoj niti bez blokiranja GUI-ja.
- [x] TXT dekodiranje za UTF-8, UTF-16 LE/BE i Windows-1252 fallback.
- [x] TXT podjela na stranice sa locatorom zasnovanim na poziciji karaktera.
- [x] EPUB container, package manifest i spine redoslijed poglavlja.
- [x] Osnovni prikaz tekstualnog sadržaja i prethodna/sljedeća navigacija.
- [x] Automatski nastavak sa posljednjeg sačuvanog locatora.
- [x] Promjena stranice ažurira progress i sekciju „Čitanje“.
- [x] Testovi za TXT kodiranja, locatore i EPUB spine redoslijed.
- [x] Očuvanje izvornog redoslijeda tekstualnih i slikovnih EPUB blokova.
- [x] EPUB JPG, PNG i WebP slike učitane isključivo iz arhive.
- [x] Inline i zasebni SVG resursi rasterizovani u memoriji.
- [x] Dekodiraju se samo slike trenutne stranice; promjena stranice oslobađa prethodni model.
- [x] Limiti veličine arhivske slike, dimenzija i memorije dekodera.
- [x] Vertikalno skrolovanje tekstualnih i slikovnih blokova unutar stranice.
- [x] URL-encoded EPUB putanje resursa.
- [x] EPUB spine dokumenti podijeljeni na čitalačke stranice umjesto prikaza poglavlja kao stranice.
- [x] SVG naslovnice koje referenciraju sliku iz EPUB arhive prikazane kao stvarna naslovna slika.
- [x] Kompatibilan nastavak čitanja sa locatora sačuvanih prije EPUB paginacije.
- [x] Automatsko razlikovanje originalnog `pre-paginated` i prilagodljivog EPUB sadržaja.
- [x] SQLite migracija verzije 4 ostaje kompatibilna sa bazama nastalim dok je postojao ručni izbor EPUB režima.
- [x] Izolovan Blitz HTML/CSS eksperiment u `prototype/epub_blitz`, bez zavisnosti glavne aplikacije.
- [x] EPUB XHTML, CSS, lokalne slike, sistemski fontovi i SVG rasterizuju se u RGBA bafer prihvatljiv Slintu.
- [x] Testna naslovnica vjerno je rasterizovana; stvarna visina sadržaja određuje broj stranica.
- [x] Procijenjena Blitz ograničenja, veličina paketa i Android/Windows prenosivost prije integracije.
  Rezultat: vjeran kontinuirani HTML/CSS prikaz i mali runtime utrošak, ali nema
  pravi paged-media prijelom; zato prototip ostaje izolovan i ne mijenja stabilni čitač.
- [x] Izolovani Qt `QTextDocument` paged-layout eksperiment, bez zavisnosti glavne aplikacije.
  Rezultat: naslovnica i stvarno prelamanje odgovaraju Okular principu, a testni
  EPUB daje 123 naspram Okularovih 124 stranice.
- [x] Ograničeno Qt učitavanje slika sa praznim layout resursima i svježim render
  dokumentom po stranici; početna i strana 120 ostaju na oko 64 MiB, uz isti raster.
- [x] Qt renderer integrisan kao opcionalni Linux desktop pomoćni proces; Slint ostaje glavni interfejs.
- [x] Automatski fallback na ugrađeni EPUB renderer kada Qt nije dostupan ili je izabran originalni režim.
- [x] Qt renderovanje radi van GUI niti, sa ograničenim kešom trenutne, prethodne i sljedeće stranice.
- [x] Izlazak iz knjige prekida čitalačku sesiju i briše njen privremeni keš čim završe aktivni render zadaci.
- [x] Uklonjen izbor tri stara EPUB režima; Linux EPUB uvijek koristi jedinstveni Qt prikaz.
- [x] Uklonjen tihi fallback za EPUB: nedostupan Qt renderer sada daje jasnu grešku umjesto drugačijeg preloma knjige.
- [x] Qt stranica se prikazuje kao jedna cjelina prilagođena dostupnoj širini i visini, bez naslijeđenog pomaka tekstualnog `ListView` prikaza.
- [x] Qt stranica ima osnovnu veličinu 600 × 800, horizontalno i vertikalno pomjeranje te zoom od 50% do 300%.
- [x] Rust bezbjedno raspakuje EPUB i Qt-u predaje pripremljeni sadržaj bez privatnog `QZipReader` API-ja.
- [x] Integracioni test potvrđuje Rust pripremu, Qt otvaranje, 123 stranice i raster prve stranice testnog EPUB-a.
- [x] Qt runtime, pomoćni renderer i potrebni plugini pakuju se uz Linux izdanje bez sistemske Qt instalacije.
- [x] Prenosivi paket ima launcher, README, Qt licence, komprimovanu arhivu i SHA-256 kontrolni zbir.
- [x] Izolovani test potvrđuje učitavanje Qt biblioteka iz paketa, EPUB render i pokretanje cijele aplikacije.
- [x] Tokom promjene stranice ostaju vidljivi zoom, navigacija i broj stranice; mijenja se samo prostor sadržaja.
- [x] Navigaciona dugmad su privremeno onemogućena dok se nova stranica renderuje.
- [x] Table of Contents sa prelaskom na izabrano poglavlje.
- [x] Pretraga unutar EPUB/TXT sadržaja i prelazak na rezultat.
- [x] Trajne oznake/bookmarks sa prelaskom na sačuvanu stranicu.
- [x] Trajna veličina fonta, tema, margine i prored za prilagodljivi tekstualni prikaz.
- [x] Desktop scroll prag i telefonski swipe gestovi.

## Faza 5 — čitač stripova

Status: završeno (`0.5.0`, proširenja `0.5.1`–`0.5.4`)

- [x] CBZ/ZIP otvaranje sa provjerom putanja, broja stavki i raspakovane veličine.
- [x] CBR/RAR otvaranje kroz statički ugrađen extraction-only UnRAR runtime.
- [x] Prirodno sortiranje stranica i podrška za JPG, PNG i WebP.
- [x] Jedna stranica sa postojećim zoom i pan/skrol kontrolama.
- [x] Režimi „Cijela stranica“, „Po širini“ i „Original/zoom“, uz fullscreen prikaz.
- [x] Ograničen dekodirani keš do 96 MiB i pozadinski preload prethodne/sljedeće stranice.
- [x] Završni testovi redoslijeda, dekodiranja, keša, oštećenih arhiva i nebezbjednih putanja.

## Faza 6 — Android integracija

Status: u toku (`0.6.0`, dopune `0.6.1`–`0.6.9`)

- [x] Slint Android backend, `cdylib` ulazna tačka i privatni direktorij aplikacije za SQLite bazu.
- [x] Android paketni metapodaci (API 26 minimum, ARM64 cilj) i provjera preduslova kroz `scripts/build_android.sh`.
- [x] Mobilni reader prepoznaje vertikalni gest: swipe gore otvara prethodnu, swipe dole sljedeću stranicu.
- [x] Režim bez kontrola: stranica zauzima cijeli prostor, a dodir je vraća; na telefonu običan dodir i swipe ostaju razdvojeni.
- [x] U režimu bez kontrola dodatni desktop skrol na dnu prelazi na sljedeću, a na vrhu na prethodnu stranicu.
- [x] Raspored navigacije, uvoza, brisanja i reader kontrola prilagođen uskom ekranu.
- [x] Tap i drag su razdvojeni: tap mijenja vidljivost kontrola, drag skroluje sadržaj, a dodatni drag na granici mijenja stranicu.
- [ ] Završna provjera svih touch interakcija na stvarnom uređaju.
- [x] SAF višestruki izbor fajlova kopira `content://` dokumente u privatni inbox i automatski ih uvozi u biblioteku.
- [x] SAF izbor čitavog foldera i rekurzivni uvoz kroz Android document tree, uz očuvanje podfoldera.
- [x] Mobilni EPUB renderer koristi ugrađeni Rust parser za spine, tekst, slike i SVG; desktop Qt prikaz ostaje nepromijenjen.
- [x] Potpisani ARM64 debug APK za ručno testiranje (`target/debug/apk/citac.apk`, API 26–35).
- [x] Optimizovani ARM64 release APK sa LTO i uklonjenim simbolima (`target/release/apk/citac.apk`, oko 7,5 MiB).
- [x] SAF kopiranje i indeksiranje rade van Android UI niti; nepotpuni fajlovi se ne uvoze, a završeni paketi se ne obrađuju ponovo poslije pokretanja.
- [x] Kompaktni mobilni raspored koristi lokalne SVG ikonice, jednu traku navigacije i manje kartice prilagođene širini Motorole G54.
- [x] Mobilni formulari, režim brisanja i reader kontrole zadržavaju najmanje 44 px dodirne mete bez horizontalnog prelivanja.
- [x] Android uvoz foldera pravi zaseban folder na početnoj strani; stripovi i knjige ostaju unutar njegove hijerarhije umjesto u ranije otvorenom folderu.

Napomena: Android build koristi postojeći SDK 35, NDK 29 i `cargo-apk`. APK je poravnat, potpisan debug certifikatom i sadrži ARM64 Slint, SQLite, CBZ i CBR kod. Fizički uređaj još nije povezan preko ADB-a, pa runtime provjera ostaje otvorena.

## Faza 7 — završna stabilizacija i pakovanje

Status: završeno (`0.6.6`–`0.6.9`)

- [x] Završna provjera performansi, keša i radnih niti.
- [x] Android SAF kopiranje velikih fajlova/foldera i SQLite indeksiranje premješteni su sa UI niti.
- [x] Android keš dekodiranih stranica stripa ograničen je na 48 MiB, dok desktop zadržava 96 MiB.
- [x] Dodat je Linux mobilni preview režim za provjeru rasporeda na uskom prozoru prije pravljenja APK-a.
- [x] Zastarjeli reader poslovi i preload susjednih stranica otkazuju se pri zatvaranju naslova ili otvaranju drugog naslova.
- [x] Uklonjen mrtvi programski interfejs i testovi za tri stara EPUB režima; postojeća SQLite kolona ostaje samo radi kompatibilnosti ranijih baza.
- [x] Prenosivi Linux paket, uključujući Qt EPUB renderer te Qt, UnRAR i Material licence; arhiva, SHA-256 i izolovano pokretanje su provjereni.
- [x] Windows paket je odvojen kao platformski zadatak jer Windows toolchain nije dostupan na ovom Linux sistemu; zajednički Rust/Slint kod nema Linux-only zavisnost osim opcionog Qt EPUB pomoćnika.
- [x] Regresioni testovi i priprema stabilne verzije.

## Faza 8 — alati čitača i personalizacija

Status: završeno (`0.8.0`–`0.8.1`)

- [x] Sadržaj knjige izveden iz EPUB poglavlja, sa mapiranjem na Qt stranice na Linuxu.
- [x] Pozadinska pretraga EPUB i TXT teksta, ograničena na 100 rezultata i zaštićena od zastarjelih rezultata.
- [x] SQLite migracija verzije 5 i jedinstvene trajne oznake po naslovu i lokatoru.
- [x] Dodavanje/uklanjanje oznake trenutne stranice i panel svih oznaka.
- [x] SQLite migracija verzije 6 za trajna podešavanja čitanja.
- [x] Veličina teksta 12–32 px, tamna/sepia/svijetla tema, margine 0–72 px i prored 1.0–2.0.
- [x] Paneli i kontrole prilagođeni mobilnom prikazu bez horizontalnog skrolovanja.
- [x] Testovi za migracije, oznake, mapiranje sadržaja/pretrage i validaciju podešavanja.
- [x] Linux paket `Citac-0.8.0-linux-x86_64.tar.gz` i potpisani ARM64 release APK 0.8.0 završno izgrađeni i provjereni.
- [x] Android 0.8.1 čuva izvorno ime svakog SAF foldera, svaki uvoz drži odvojeno i koristi immersive fullscreen bez sistemskih traka.

## Faza 9 — nazivi i naslovnice biblioteke

Status: završeno (`0.9.0`)

- [x] Kod pojedinačnog Android SAF uvoza prikazuje se originalno ime knjige ili stripa bez internog brojčanog prefiksa.
- [x] Jedinstveni interni naziv kopiranog fajla više nije vidljiv korisniku.
- [x] Podržani su pojedinačni fajlovi istog imena, a raniji Android naslovi sa dugim brojčanim prefiksom bezbjedno se popravljaju.
- [x] EPUB koristi prvu pronađenu odgovarajuću sliku naslovnice.
- [x] CBZ i CBR koriste prvu stranicu kao thumbnail.
- [x] TXT dobija jednostavnu generisanu naslovnicu sa nazivom.
- [x] Thumbnaili se smanjuju na najviše 240 × 360 px i keširaju izvan baze.
- [x] Svi folderi koriste istu statičnu ikonicu bez obzira na sadržaj; mozaik naslovnica se ne pravi.
- [x] SQLite migracija verzije 7 pamti putanju thumbnaila i izbjegava ponovno otvaranje arhive pri svakom prikazu.
- [x] Mobilni prikaz kartica provjeren je na virtuelnom ekranu širine 420 px.
- [x] Svih 31 regresionih testova prolazi; Linux 0.9.0 paket i potpisani ARM64 Android 0.9.0 APK su izgrađeni i provjereni.

## Faza 10 — napredni režimi čitanja stripova

Status: završeno (`0.10.0`)

- [x] Prikaz jedne cijele stranice, po širini, po visini i u originalnoj veličini sa zoomom.
- [x] Prikaz dvije susjedne stranice sa prelaskom po dva lista.
- [x] Vertikalni prikaz dva uzastopna lista sa skrolovanjem i nastavkom na sljedeći par pri granici.
- [x] Susjedna stranica se dekodira u pozadini i koristi postojeći memorijski ograničeni comic keš.
- [x] Direktan odlazak na uneseni broj stranice uz provjeru raspona.
- [x] Brojač prikazuje raspon stranica u dvostrukom i vertikalnom režimu.
- [x] Swipe, točkić miša, skrivanje kontrola, oznake i pamćenje napretka ostaju povezani sa aktivnom stranicom.
- [x] Kontrole su raspoređene u dva reda kako bi ostale upotrebljive na uskom Android ekranu.
- [x] Svih 31 regresionih testova prolazi; Linux 0.10.0 paket i potpisani ARM64 Android 0.10.0 APK su izgrađeni i provjereni.

## Faza 11 — skalabilna Android biblioteka i grid

Status: završeno (`0.11.0`–`0.11.2`)

- [x] Biblioteka i sadržaj foldera prikazuju se kao grid sa dvije kolone na telefonu.
- [x] Naslovnica je strogo ograničena i isječena unutar kartice; dugi naslovi imaju fiksan prostor i elipsu.
- [x] Folderi zadržavaju jedinstvenu statičnu ikonicu bez mozaika.
- [x] Thumbnail keš koristi JPEG kvaliteta 72 i maksimalno 200 × 300 px radi manjeg zauzeća.
- [x] Android SAF više ne kopira novoizabrane knjige i stripove u privatni prostor aplikacije.
- [x] Čuvaju se trajne SAF URI dozvole i mali tekstualni indeks foldera, podfoldera i naslova.
- [x] Originalni dokument se pri čitanju i izradi thumbnaila otvara preko Android file descriptora.
- [x] Direktni URI naslovi i pripadnost folderima čuvaju se u SQLite bazi bez dupliranja izvornog sadržaja.
- [x] Uklonjeni su comic režimi original/zoom i vertikalno; ostaju cijela stranica, po širini, po visini i dvije stranice.
- [x] Režimi prikaza koriste lokalne SVG ikonice umjesto tekstualnih naziva.
- [x] Poruka o završenom uvozu briše se pri ulasku u folder/naslov i pri povratku.
- [x] Dodan regresioni test koji potvrđuje da Android URI ostaje izvor naslova bez kopiranja dokumenta.
- [x] Svih 32 regresionih testova prolazi; Linux 0.11.0 paket i potpisani ARM64 Android 0.11.0 APK su izgrađeni i provjereni.
- [x] Android 0.11.1 materijalizuje samo trenutno otvoreni SAF dokument u privremeni cache radi pouzdanog ZIP/RAR pristupa i briše ga pri zatvaranju ili narednom pokretanju.
- [x] Dvostruki prikaz ima dvije ravnopravne, isječene površine pa se susjedne stranice ne mogu preklapati.
- [x] Android 0.11.2 prikazuje obje stranice po punoj širini njihove polovine ekrana, uz zajednički vertikalni skrol i okvir reader dugmadi usklađen sa pozadinom.

## Faza 12 — engleski interfejs i bezbjedna zona ekrana

Status: završeno (`0.12.0`)

- [x] Svi korisnički tekstovi, poruke, dijalozi i greške prevedeni su na engleski jezik.
- [x] Naziv aplikacije na početnoj strani je skriven, ali je sačuvan isti vertikalni razmak.
- [x] Ekran foldera i čitača na telefonu imaju povećan gornji razmak kako sadržaj ne bi ulazio u zonu kamere.
- [x] Naziv Android aplikacije i podrazumijevani probni sadržaj prevedeni su na engleski.

## Faza 13 — ikonica i instalacioni paketi

Status: završeno (`0.13.0`)

- [x] Nova zajednička ikonica povezana je sa prozorom aplikacije i paketima za sve platforme.
- [x] Android launcher koristi novu ikonicu iz APK resursa.
- [x] Linux paket sadrži ikonicu, desktop unos i instalaciju za trenutnog korisnika.
- [x] Windows izvršni fajl sadrži ugrađenu ikonicu, a paket uključuje instalaciju i deinstalaciju bez administratorskih prava.
- [x] Pripremljeni su potpisani Android APK, provjereni Linux paket i strukturalno provjeren Windows x86-64 paket.

## Planirano za naredni rad

- [ ] Testirati direktni SAF uvoz i otvaranje CBZ, CBR, EPUB i TXT dokumenata na stvarnom Android uređaju.
- [ ] Nakon potvrde obezbijediti bezbjedno čišćenje starih privatnih kopija nastalih u verzijama do 0.10.0.

## Napomene i poznata ograničenja

- Probni naslovi su sada u lokalnoj SQLite bazi, ali još nisu povezani sa stvarnim fajlovima.
- Stvarno uvezeni naslovi čuvaju kanonsku putanju, veličinu, vrijeme izmjene i SHA-256 sadržaja; metadata autora biće dopunjena u fazi parsera.
- EPUB i TXT sadržaj se parsira; CBZ i CBR čitač dolazi u fazi 5.
- Potpuno vjeran originalni prikaz moguć je kada EPUB sadrži `pre-paginated` stranice ili po jednu cijelu sliku/SVG po spine stavci; obični reflowable EPUB nema ugrađene fiksne stranice.
- Android izvorni build, SAF uvoz i ARM64 APK su dostupni; runtime provjera mobilnog EPUB prikaza ostaje otvorena.
- SQLite baza se čuva u platformskom direktorijumu za podatke koji određuje `directories` crate.
