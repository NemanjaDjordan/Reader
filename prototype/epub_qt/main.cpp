#include <QAbstractTextDocumentLayout>
#include <QCoreApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImage>
#include <QImageReader>
#include <QPainter>
#include <QRegularExpression>
#include <QSet>
#include <QTextBlock>
#include <QTextBlockFormat>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextFragment>
#include <QTextImageFormat>
#include <QUrl>
#include <QVariant>

#include <algorithm>
#include <cmath>
#include <iostream>
#include <memory>
#include <optional>
#include <utility>

namespace {

constexpr int kPageWidth = 600;
constexpr int kPageHeight = 800;
constexpr int kContentPadding = 20;
struct EpubPackage {
    QString root;
    QStringList spine;
};

QSize constrainedImageSize(const QString &path) {
    QImageReader reader(path);
    reader.setAutoTransform(true);
    QSize size = reader.size();
    if (!size.isValid()) {
        return {};
    }
    const QSize maximum(kPageWidth - 2 * kContentPadding,
                        kPageHeight - 2 * kContentPadding);
    if (size.width() > maximum.width() || size.height() > maximum.height()) {
        size.scale(maximum, Qt::KeepAspectRatio);
    }
    return size;
}

class LocalTextDocument final : public QTextDocument {
public:
    LocalTextDocument(QString allowedRoot, qreal fontPointSize)
        : allowedRoot_(QFileInfo(std::move(allowedRoot)).canonicalFilePath())
        , fontPointSize_(fontPointSize) {}

    void setCurrentSubDocument(const QString &path) {
        currentBase_ = QUrl::fromLocalFile(QFileInfo(path).absolutePath() + '/');
        setBaseUrl(currentBase_);
    }

    QString checkCss(QString css) const {
        static const QRegularExpression lineHeight(
            QStringLiteral(R"(line-height\s*:\s*[\w\.]*;?)"),
            QRegularExpression::CaseInsensitiveOption);
        css.remove(lineHeight);

        static const QRegularExpression emValue(
            QStringLiteral(R"(([0-9]+(?:\.[0-9]+)?)r?em\b)"),
            QRegularExpression::CaseInsensitiveOption);
        qsizetype offset = 0;
        while (true) {
            const auto match = emValue.match(css, offset);
            if (!match.hasMatch()) {
                break;
            }
            const QString replacement = QString::number(
                                            match.captured(1).toDouble() * fontPointSize_, 'f', 3)
                + QStringLiteral("px");
            css.replace(match.capturedStart(), match.capturedLength(), replacement);
            offset = match.capturedStart() + replacement.size();
        }
        return css;
    }

    QSet<QUrl> imageUrlsForPage(int page) const {
        const qreal top = page * kPageHeight;
        const qreal bottom = top + kPageHeight;
        QSet<QUrl> required;

        for (QTextBlock block = begin(); block.isValid(); block = block.next()) {
            const QRectF bounds = documentLayout()->blockBoundingRect(block);
            if (bounds.bottom() < top || bounds.top() >= bottom) {
                continue;
            }
            for (QTextBlock::iterator iterator = block.begin(); !iterator.atEnd(); ++iterator) {
                const QTextFragment fragment = iterator.fragment();
                if (!fragment.isValid() || !fragment.charFormat().isImageFormat()) {
                    continue;
                }
                const QTextImageFormat format = fragment.charFormat().toImageFormat();
                const QUrl url(format.name());
                if (url.isLocalFile()) {
                    required.insert(url);
                }
            }
        }
        return required;
    }

    QImage decodedImage(const QUrl &url) const {
        QImageReader reader(url.toLocalFile());
        reader.setAutoTransform(true);
        const QSize size = constrainedImageSize(url.toLocalFile());
        if (size.isValid() && size != reader.size()) {
            reader.setScaledSize(size);
        }
        return reader.read();
    }

protected:
    QVariant loadResource(int type, const QUrl &name) override {
        const QUrl resolved = name.isRelative() ? currentBase_.resolved(name) : name;
        if (!resolved.isLocalFile()) {
            return {};
        }

        const QString canonical = QFileInfo(resolved.toLocalFile()).canonicalFilePath();
        const QString prefix = allowedRoot_ + QDir::separator();
        if (canonical.isEmpty()
            || (canonical != allowedRoot_ && !canonical.startsWith(prefix))) {
            return {};
        }

        if (type == QTextDocument::ImageResource) {
            const QImage placeholder;
            addResource(type, name, placeholder);
            return placeholder;
        }

        QFile file(canonical);
        if (!file.open(QIODevice::ReadOnly)) {
            return {};
        }
        const QByteArray bytes = file.readAll();
        if (type == QTextDocument::StyleSheetResource) {
            return checkCss(QString::fromUtf8(bytes));
        }
        return bytes;
    }

private:
    QString allowedRoot_;
    QUrl currentBase_;
    qreal fontPointSize_;
};

bool fail(const QString &message) {
    std::cerr << message.toStdString() << '\n';
    return false;
}

std::optional<QString> safeRelativePath(QString path) {
    path.replace('\\', '/');
    if (path.isEmpty() || path.startsWith('/')
        || QRegularExpression(QStringLiteral("^[A-Za-z]:")).match(path).hasMatch()) {
        return std::nullopt;
    }
    const QString clean = QDir::cleanPath(path);
    if (clean == QStringLiteral("..") || clean.startsWith(QStringLiteral("../"))) {
        return std::nullopt;
    }
    return clean;
}

std::optional<QByteArray> readFile(const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        return std::nullopt;
    }
    return file.readAll();
}

std::optional<EpubPackage> openPreparedEpub(const QString &rootPath,
                                            const QString &spineListPath) {
    const QString root = QFileInfo(rootPath).canonicalFilePath();
    const auto rawList = readFile(spineListPath);
    if (root.isEmpty() || !rawList) {
        fail(QStringLiteral("Pripremljeni EPUB direktorijum ili spine lista nisu dostupni"));
        return std::nullopt;
    }

    QStringList spine;
    const QString rootPrefix = root + QDir::separator();
    for (const QByteArray &line : rawList->split('\n')) {
        if (line.trimmed().isEmpty()) {
            continue;
        }
        const auto relative = safeRelativePath(QString::fromUtf8(line.trimmed()));
        if (!relative) {
            fail(QStringLiteral("Nebezbjedna putanja u pripremljenoj spine listi"));
            return std::nullopt;
        }
        const QString canonical = QFileInfo(QDir(root).filePath(*relative)).canonicalFilePath();
        if (canonical.isEmpty() || !canonical.startsWith(rootPrefix)) {
            fail(QStringLiteral("Spine dokument izlazi iz pripremljenog EPUB direktorijuma"));
            return std::nullopt;
        }
        spine.append(canonical);
    }
    if (spine.isEmpty()) {
        fail(QStringLiteral("EPUB spine je prazan"));
        return std::nullopt;
    }
    return EpubPackage{root, spine};
}

QString replaceSvgImageWrappers(QString html) {
    static const QRegularExpression svgExpression(
        QStringLiteral(R"(<svg\b([^>]*)>\s*<image\b([^>]*)/?>\s*</svg\s*>)"),
        QRegularExpression::CaseInsensitiveOption | QRegularExpression::DotMatchesEverythingOption);
    static const QRegularExpression hrefExpression(
        QStringLiteral(R"((?:xlink:)?href\s*=\s*["']([^"']+)["'])"),
        QRegularExpression::CaseInsensitiveOption);

    qsizetype offset = 0;
    while (true) {
        const auto svgMatch = svgExpression.match(html, offset);
        if (!svgMatch.hasMatch()) {
            break;
        }
        const QString imageAttributes = svgMatch.captured(2);
        const auto hrefMatch = hrefExpression.match(imageAttributes);
        if (!hrefMatch.hasMatch()) {
            offset = svgMatch.capturedEnd();
            continue;
        }
        QString href = hrefMatch.captured(1).toHtmlEscaped();
        const auto dimension = [&imageAttributes](const QString &name) {
            const QRegularExpression expression(
                QStringLiteral(R"(%1\s*=\s*["']([0-9]+(?:\.[0-9]+)?)["'])").arg(name),
                QRegularExpression::CaseInsensitiveOption);
            return expression.match(imageAttributes).captured(1).toDouble();
        };
        const int width = std::min(kPageWidth - 2 * kContentPadding,
                                   static_cast<int>(std::round(dimension(QStringLiteral("width")))));
        const int height = std::min(kPageHeight - 2 * kContentPadding,
                                    static_cast<int>(std::round(dimension(QStringLiteral("height")))));
        const QString image = QStringLiteral(R"(<img src="%1" width="%2" height="%3"/>)")
                                  .arg(href)
                                  .arg(width > 0 ? width : kPageWidth - 2 * kContentPadding)
                                  .arg(height > 0 ? height : kPageHeight - 2 * kContentPadding);
        html.replace(svgMatch.capturedStart(), svgMatch.capturedLength(), image);
        offset = svgMatch.capturedStart() + image.size();
    }
    return html;
}

QString absolutizeLocalImages(QString html, const QString &documentPath,
                              const QString &allowedRoot) {
    static const QRegularExpression imageExpression(
        QStringLiteral(R"(<img\b[^>]*>)"),
        QRegularExpression::CaseInsensitiveOption);
    static const QRegularExpression sourceExpression(
        QStringLiteral(R"(\bsrc\s*=\s*["']([^"']+)["'])"),
        QRegularExpression::CaseInsensitiveOption);
    static const QRegularExpression widthExpression(
        QStringLiteral(R"(\bwidth\s*=)"), QRegularExpression::CaseInsensitiveOption);
    static const QRegularExpression heightExpression(
        QStringLiteral(R"(\bheight\s*=)"), QRegularExpression::CaseInsensitiveOption);
    const QUrl base = QUrl::fromLocalFile(QFileInfo(documentPath).absolutePath() + '/');
    const QString root = QFileInfo(allowedRoot).canonicalFilePath();
    const QString prefix = root + QDir::separator();
    QList<QPair<QPair<qsizetype, qsizetype>, QString>> replacements;

    auto matches = imageExpression.globalMatch(html);
    while (matches.hasNext()) {
        const auto match = matches.next();
        QString tag = match.captured(0);
        const auto source = sourceExpression.match(tag);
        if (!source.hasMatch()) {
            continue;
        }
        const QUrl resolved = base.resolved(QUrl(source.captured(1)));
        if (!resolved.isLocalFile()) {
            continue;
        }
        const QString canonical = QFileInfo(resolved.toLocalFile()).canonicalFilePath();
        if (canonical.isEmpty() || (canonical != root && !canonical.startsWith(prefix))) {
            continue;
        }
        const QString absoluteUrl = QUrl::fromLocalFile(canonical)
                                        .toString(QUrl::FullyEncoded)
                                        .toHtmlEscaped();
        tag.replace(source.capturedStart(1), source.capturedLength(1), absoluteUrl);

        const QSize imageSize = constrainedImageSize(canonical);
        QString dimensions;
        if (imageSize.isValid() && !widthExpression.match(tag).hasMatch()) {
            dimensions += QStringLiteral(" width=\"%1\"").arg(imageSize.width());
        }
        if (imageSize.isValid() && !heightExpression.match(tag).hasMatch()) {
            dimensions += QStringLiteral(" height=\"%1\"").arg(imageSize.height());
        }
        const qsizetype close = tag.endsWith(QStringLiteral("/>")) ? tag.size() - 2
                                                                    : tag.size() - 1;
        tag.insert(close, dimensions);
        replacements.append({{match.capturedStart(), match.capturedLength()}, tag});
    }
    for (auto iterator = replacements.crbegin(); iterator != replacements.crend(); ++iterator) {
        html.replace(iterator->first.first, iterator->first.second, iterator->second);
    }
    return html;
}

QString inlineLocalStylesheets(QString html, const QString &documentPath,
                               const QString &allowedRoot, const LocalTextDocument &document) {
    static const QRegularExpression linkExpression(
        QStringLiteral(R"(<link\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>)"),
        QRegularExpression::CaseInsensitiveOption);
    QString styles;
    auto match = linkExpression.globalMatch(html);
    while (match.hasNext()) {
        const auto current = match.next();
        const QString href = QUrl::fromPercentEncoding(
            current.captured(1).section('#', 0, 0).section('?', 0, 0).toUtf8());
        const auto relative = safeRelativePath(href);
        if (!relative) {
            continue;
        }
        const QString path = QDir(QFileInfo(documentPath).absolutePath()).filePath(*relative);
        const QString canonical = QFileInfo(path).canonicalFilePath();
        const QString root = QFileInfo(allowedRoot).canonicalFilePath();
        if (canonical.isEmpty()
            || (canonical != root && !canonical.startsWith(root + QDir::separator()))) {
            continue;
        }
        if (const auto css = readFile(canonical)) {
            styles += document.checkCss(QString::fromUtf8(*css));
            styles += '\n';
        }
    }
    if (!styles.isEmpty()) {
        html.prepend(QStringLiteral("<style>") + styles + QStringLiteral("</style>"));
    }
    return html;
}

bool appendChapter(LocalTextDocument &document, QTextCursor &cursor, bool first,
                   const QString &path, const QString &allowedRoot) {
    const auto source = readFile(path);
    if (!source) {
        fail(QStringLiteral("Nije moguće učitati spine dokument: %1").arg(path));
        return false;
    }

    QString html = QString::fromUtf8(*source);
    html = replaceSvgImageWrappers(std::move(html));
    html = absolutizeLocalImages(std::move(html), path, allowedRoot);
    html = inlineLocalStylesheets(std::move(html), path, allowedRoot, document);

    document.setCurrentSubDocument(path);
    if (first) {
        document.setHtml(html);
        cursor = QTextCursor(&document);
        cursor.movePosition(QTextCursor::End);
    } else {
        cursor.insertHtml(html);
    }

    const int contentPageCount = document.pageCount();
    cursor.insertBlock(QTextBlockFormat());
    while (document.pageCount() == contentPageCount) {
        cursor.insertText(QStringLiteral("\n"));
    }
    return true;
}

std::unique_ptr<LocalTextDocument> layoutBook(const EpubPackage &package,
                                              qreal fontSize) {
    auto document = std::make_unique<LocalTextDocument>(package.root, fontSize);
    document->setPageSize(QSizeF(kPageWidth, kPageHeight));
    document->setDefaultStyleSheet(QStringLiteral("a { color: #0000ff; }"));
    QTextCursor cursor(document.get());
    bool first = true;
    for (const QString &path : package.spine) {
        if (!appendChapter(*document, cursor, first, path, package.root)) {
            return {};
        }
        first = false;
    }
    return document;
}

bool renderPage(const EpubPackage &package, qreal fontSize, int page,
                const QString &outputPath, int &loadedImages) {
    std::unique_ptr<LocalTextDocument> pageDocument = layoutBook(package, fontSize);
    if (!pageDocument) {
        return false;
    }
    const QSet<QUrl> activeImages = pageDocument->imageUrlsForPage(page);
    for (const QUrl &url : activeImages) {
        pageDocument->addResource(QTextDocument::ImageResource, url,
                                  pageDocument->decodedImage(url));
    }
    loadedImages = activeImages.size();

    QImage image(kPageWidth, kPageHeight, QImage::Format_RGBA8888);
    image.fill(Qt::white);

    QPainter painter(&image);
    painter.translate(0, -page * kPageHeight);
    QAbstractTextDocumentLayout::PaintContext context;
    context.clip = QRectF(0, page * kPageHeight, kPageWidth, kPageHeight);
    pageDocument->documentLayout()->draw(&painter, context);
    painter.end();

    if (image.bytesPerLine() != kPageWidth * 4) {
        return fail(QStringLiteral("RGBA bafer nema očekivani stride"));
    }
    if (!image.save(outputPath, "PNG")) {
        return fail(QStringLiteral("Nije moguće snimiti: %1").arg(outputPath));
    }
    return true;
}

} // namespace

int main(int argc, char *argv[]) {
    if (qEnvironmentVariableIsEmpty("QT_QPA_PLATFORM")
        && qEnvironmentVariableIsEmpty("DISPLAY")
        && qEnvironmentVariableIsEmpty("WAYLAND_DISPLAY")) {
        qputenv("QT_QPA_PLATFORM", "offscreen");
    }
    QGuiApplication application(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("citac-epub-qt-prototype"));
    QImageReader::setAllocationLimit(64);

    if (argc < 4) {
        fail(QStringLiteral(
            "Upotreba: renderer <raspakovani-root> <spine-lista> <izlaz> [broj] [font] [prva]"));
        return 2;
    }
    const QString preparedRoot = QFileInfo(QString::fromLocal8Bit(argv[1])).absoluteFilePath();
    const QString spineList = QFileInfo(QString::fromLocal8Bit(argv[2])).absoluteFilePath();
    const QString outputDir = QFileInfo(QString::fromLocal8Bit(argv[3])).absoluteFilePath();
    const int renderLimit = argc > 4
                                ? std::max(0, QString::fromLocal8Bit(argv[4]).toInt())
                                : 3;
    constexpr qreal defaultReaderFontSize = 10.0;
    const qreal fontSize = argc > 5 ? QString::fromLocal8Bit(argv[5]).toDouble()
                                    : defaultReaderFontSize;
    const int firstRenderedPage = argc > 6
                                      ? std::max(0, QString::fromLocal8Bit(argv[6]).toInt() - 1)
                                      : 0;

    if (!QDir().mkpath(outputDir)) {
        fail(QStringLiteral("Nije moguće napraviti izlazni direktorijum"));
        return 1;
    }

    QElapsedTimer timer;
    timer.start();
    auto package = openPreparedEpub(preparedRoot, spineList);
    if (!package) {
        return 1;
    }

    std::cout << "Pripremljeni EPUB: " << preparedRoot.toStdString() << '\n';
    std::cout << "Spine dokumenata: " << package->spine.size() << '\n';
    std::cout << "Osnovni font: " << QString::number(fontSize, 'f', 3).toStdString()
              << "pt\n";

    std::unique_ptr<LocalTextDocument> document = layoutBook(*package, fontSize);
    if (!document) {
        return 1;
    }

    const int globalPage = document->pageCount();
    const int renderEnd = std::min(firstRenderedPage + renderLimit, globalPage);
    for (int page = firstRenderedPage; page < renderEnd; ++page) {
        const QString outputPath = QDir(outputDir).filePath(
            QStringLiteral("stranica-%1.png").arg(page + 1, 3, 10, QLatin1Char('0')));
        int loadedImages = 0;
        if (!renderPage(*package, fontSize, page, outputPath, loadedImages)) {
            return 1;
        }
        std::cout << QStringLiteral(
                         "  rasterizovana stranica %1: %2 (RGBA8888 %3x%4, aktivnih slika: %5)")
                         .arg(page + 1)
                         .arg(outputPath)
                         .arg(kPageWidth)
                         .arg(kPageHeight)
                         .arg(loadedImages)
                         .toStdString()
                  << '\n';
    }

    std::cout << QStringLiteral("Izmjerenih stranica %1x%2: %3")
                     .arg(kPageWidth)
                     .arg(kPageHeight)
                     .arg(globalPage)
                     .toStdString()
              << '\n';
    std::cout << "CITAC_RESULT pages=" << globalPage << '\n';
    std::cout << "Ukupno vrijeme prototipa: "
              << QString::number(timer.elapsed() / 1000.0, 'f', 3).toStdString() << "s\n";
    return 0;
}
