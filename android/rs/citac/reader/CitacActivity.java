package rs.citac.reader;

import android.app.NativeActivity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.provider.DocumentsContract;
import android.os.ParcelFileDescriptor;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

public class CitacActivity extends NativeActivity {
    private static final int OPEN_DOCUMENTS = 4101;
    private static final int OPEN_FOLDER = 4102;
    private final Map<Integer, ParcelFileDescriptor> openDocuments =
            new ConcurrentHashMap<Integer, ParcelFileDescriptor>();
    private final Map<Integer, File> temporaryDocuments =
            new ConcurrentHashMap<Integer, File>();

    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        clearStaleOpenDocuments();
        enableImmersiveFullscreen();
    }

    private void clearStaleOpenDocuments() {
        File directory = new File(getCacheDir(), "open-documents");
        File[] files = directory.listFiles();
        if (files == null) return;
        for (File file : files) {
            if (file.isFile()) file.delete();
        }
    }

    @Override protected void onResume() {
        super.onResume();
        enableImmersiveFullscreen();
    }

    @Override public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) enableImmersiveFullscreen();
    }

    private void enableImmersiveFullscreen() {
        final Window window = getWindow();
        if (Build.VERSION.SDK_INT >= 28) {
            WindowManager.LayoutParams attributes = window.getAttributes();
            attributes.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            window.setAttributes(attributes);
        }
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false);
            WindowInsetsController controller = window.getInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.statusBars() |
                        WindowInsets.Type.navigationBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            window.getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY |
                    View.SYSTEM_UI_FLAG_FULLSCREEN |
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION |
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE |
                    View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN |
                    View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }
    }

    public void openDocuments() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION |
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
        startActivityForResult(intent, OPEN_DOCUMENTS);
    }

    public void openFolder() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION |
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION |
                Intent.FLAG_GRANT_PREFIX_URI_PERMISSION);
        startActivityForResult(intent, OPEN_FOLDER);
    }

    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (result != RESULT_OK || data == null) return;
        if (request == OPEN_FOLDER && data.getData() != null) {
            final Uri tree = data.getData();
            try {
                getContentResolver().takePersistableUriPermission(tree,
                        Intent.FLAG_GRANT_READ_URI_PERMISSION);
            } catch (Exception ignored) {}
            new Thread(new Runnable() {
                @Override public void run() {
                    File batch = new File(getFilesDir(), "saf-reference-inbox/" +
                            System.currentTimeMillis() + "-" + System.nanoTime());
                    batch.mkdirs();
                    String rootName = treeDocumentName(tree);
                    writeText(new File(batch, ".root-name"), rootName);
                    indexDocumentTree(tree, DocumentsContract.getTreeDocumentId(tree),
                            "", batch, new int[] {0}, 0);
                    try { new File(batch, ".ready").createNewFile(); } catch (Exception ignored) {}
                }
            }, "citac-saf-folder-copy").start();
            return;
        }
        if (request != OPEN_DOCUMENTS) return;
        final List<Uri> documents = new ArrayList<Uri>();
        if (data.getClipData() != null) {
            for (int i = 0; i < data.getClipData().getItemCount(); i++) {
                documents.add(data.getClipData().getItemAt(i).getUri());
            }
        } else if (data.getData() != null) {
            documents.add(data.getData());
        }
        for (Uri document : documents) {
            try {
                getContentResolver().takePersistableUriPermission(document,
                        Intent.FLAG_GRANT_READ_URI_PERMISSION);
            } catch (Exception ignored) {}
        }
        new Thread(new Runnable() {
            @Override public void run() {
                File batch = new File(getFilesDir(), "saf-reference-inbox/" +
                        System.currentTimeMillis() + "-" + System.nanoTime());
                batch.mkdirs();
                int index = 0;
                for (Uri document : documents) {
                    writeReference(document, documentName(document), "", batch, index++);
                }
                try { new File(batch, ".ready").createNewFile(); } catch (Exception ignored) {}
            }
        }, "citac-saf-file-copy").start();
    }

    public int openDocumentFd(String uriText) {
        File temporary = null;
        try {
            Uri uri = Uri.parse(uriText);
            File directory = new File(getCacheDir(), "open-documents");
            directory.mkdirs();
            String name = safeFileName(documentName(uri));
            temporary = new File(directory, System.nanoTime() + "-" + name);
            try (InputStream input = getContentResolver().openInputStream(uri);
                 FileOutputStream output = new FileOutputStream(temporary)) {
                if (input == null) return -1;
                byte[] buffer = new byte[131072];
                int count;
                while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
                output.getFD().sync();
            }
            ParcelFileDescriptor descriptor = ParcelFileDescriptor.open(
                    temporary, ParcelFileDescriptor.MODE_READ_ONLY);
            if (descriptor == null) return -1;
            int fd = descriptor.getFd();
            openDocuments.put(fd, descriptor);
            temporaryDocuments.put(fd, temporary);
            return fd;
        } catch (Exception ignored) {
            if (temporary != null) temporary.delete();
            return -1;
        }
    }

    public void closeDocumentFd(int fd) {
        ParcelFileDescriptor descriptor = openDocuments.remove(fd);
        if (descriptor != null) {
            try { descriptor.close(); } catch (Exception ignored) {}
        }
        File temporary = temporaryDocuments.remove(fd);
        if (temporary != null) temporary.delete();
    }

    private String documentName(Uri uri) {
        String[] columns = { DocumentsContract.Document.COLUMN_DISPLAY_NAME };
        try (Cursor cursor = getContentResolver().query(uri, columns, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) return cursor.getString(0);
        } catch (Exception ignored) {}
        return "Imported folder";
    }

    private String treeDocumentName(Uri tree) {
        try {
            String rootId = DocumentsContract.getTreeDocumentId(tree);
            Uri rootDocument = DocumentsContract.buildDocumentUriUsingTree(tree, rootId);
            String name = documentName(rootDocument);
            if (name != null && !name.trim().isEmpty() &&
                    !"Imported folder".equals(name)) return safeFileName(name);

            int separator = rootId.lastIndexOf('/');
            String fallback = separator >= 0 ? rootId.substring(separator + 1) : rootId;
            int volume = fallback.lastIndexOf(':');
            if (volume >= 0) fallback = fallback.substring(volume + 1);
            if (!fallback.trim().isEmpty()) return safeFileName(fallback);
        } catch (Exception ignored) {}
        return "Imported folder";
    }

    private String safeFileName(String name) {
        String safe = name == null ? "" : name
                .replaceAll("[\\\\/:*?\"<>|\\p{Cntrl}]", "_")
                .trim();
        return safe.isEmpty() ? "Imported folder" : safe;
    }

    private void indexDocumentTree(Uri tree, String documentId, String relative,
                                   File batch, int[] index, int depth) {
        if (depth > 32) return;
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, documentId);
        String[] columns = {
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE
        };
        try (Cursor cursor = getContentResolver().query(children, columns, null, null, null)) {
            if (cursor == null) return;
            while (cursor.moveToNext()) {
                String childId = cursor.getString(0);
                String name = safeFileName(cursor.getString(1));
                String mime = cursor.getString(2);
                Uri child = DocumentsContract.buildDocumentUriUsingTree(tree, childId);
                if (DocumentsContract.Document.MIME_TYPE_DIR.equals(mime)) {
                    String childRelative = relative.isEmpty() ? name : relative + "/" + name;
                    indexDocumentTree(tree, childId, childRelative, batch, index, depth + 1);
                } else {
                    writeReference(child, name, relative, batch, index[0]++);
                }
            }
        } catch (Exception ignored) {}
    }

    private void writeReference(Uri uri, String name, String relative, File batch, int index) {
        String contents = uri.toString() + "\n" + safeLine(name) + "\n" +
                safeLine(relative) + "\n";
        writeText(new File(batch, String.format("%08d.ref", index)), contents);
    }

    private String safeLine(String value) {
        return value == null ? "" : value.replace('\n', ' ').replace('\r', ' ');
    }

    private void writeText(File output, String contents) {
        try (FileOutputStream stream = new FileOutputStream(output)) {
            stream.write(contents.getBytes(StandardCharsets.UTF_8));
            stream.getFD().sync();
        } catch (Exception error) {
            output.delete();
        }
    }

    private void copyIntoInbox(Uri uri, File inbox) {
        String name = "dokument-" + System.currentTimeMillis();
        try (Cursor cursor = getContentResolver().query(uri, null, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                int column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (column >= 0) name = cursor.getString(column);
            }
        } catch (Exception ignored) {}
        name = safeFileName(name);
        File output = new File(inbox, System.currentTimeMillis() + "-" + name);
        int suffix = 1;
        while (output.exists() || new File(output.getPath() + ".part").exists()) {
            output = new File(inbox, System.currentTimeMillis() + "-" + suffix + "-" + name);
            suffix++;
        }
        File temporary = new File(output.getPath() + ".part");
        try (InputStream input = getContentResolver().openInputStream(uri);
             FileOutputStream stream = new FileOutputStream(temporary)) {
            if (input == null) {
                temporary.delete();
                return;
            }
            byte[] buffer = new byte[65536];
            int count;
            while ((count = input.read(buffer)) != -1) stream.write(buffer, 0, count);
            stream.getFD().sync();
        } catch (Exception error) {
            temporary.delete();
            return;
        }
        if (!temporary.renameTo(output)) {
            temporary.delete();
            return;
        }
        File nameFile = new File(output.getPath() + ".name");
        try (FileOutputStream stream = new FileOutputStream(nameFile)) {
            stream.write(name.getBytes(StandardCharsets.UTF_8));
            stream.getFD().sync();
        } catch (Exception error) {
            nameFile.delete();
        }
    }
}
