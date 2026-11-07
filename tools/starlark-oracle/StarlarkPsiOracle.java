import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

import com.intellij.core.CoreApplicationEnvironment;
import com.intellij.core.CoreProjectEnvironment;
import com.intellij.openapi.Disposable;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.util.Disposer;
import com.intellij.openapi.util.text.StringUtil;
import com.intellij.psi.PsiFile;
import com.intellij.psi.PsiFileFactory;
import com.intellij.psi.impl.DebugUtil;
import com.intellij.psi.impl.PsiFileFactoryImpl;
import com.intellij.testFramework.LightVirtualFile;

import org.jetbrains.bazel.languages.starlark.StarlarkLanguage;
import org.jetbrains.bazel.languages.starlark.StarlarkFileType;
import org.jetbrains.bazel.languages.starlark.parser.StarlarkParserDefinition;

public class StarlarkPsiOracle {
  public static void main(String[] args) throws Exception {
    Path list = null;
    Path root = Path.of(".");
    Path out = null;
    boolean trim = false;
    for (int i = 0; i < args.length; i++) {
      switch (args[i]) {
        case "--list" -> list = Path.of(args[++i]);
        case "--root" -> root = Path.of(args[++i]);
        case "--out" -> out = Path.of(args[++i]);
        case "--trim" -> trim = true;
        default -> throw new IllegalArgumentException("unknown argument " + args[i]);
      }
    }
    if (list == null || out == null) throw new IllegalArgumentException("--list and --out are required");
    Path listFile = list;
    Path rootDir = root;
    Path outDir = out;
    boolean trimText = trim;
    Throwable[] failure = new Throwable[1];
    Thread worker = new Thread(null, () -> {
      try {
        Disposable disposable = Disposer.newDisposable();
        try {
          Project project = createProject(disposable);
          run(project, Files.readAllLines(listFile, StandardCharsets.UTF_8), rootDir, outDir, trimText);
        }
        finally {
          Disposer.dispose(disposable);
        }
      }
      catch (Throwable t) {
        failure[0] = t;
      }
    }, "starlark-psi-oracle", 1L << 30);
    worker.start();
    worker.join();
    if (failure[0] != null) {
      failure[0].printStackTrace();
      System.exit(1);
    }
  }

  private static Project createProject(Disposable disposable) throws IOException {
    if (System.getProperty("idea.home.path") == null) {
      Path home = Path.of(System.getProperty("java.io.tmpdir"), "psi-syntax-starlark-oracle-home");
      Files.createDirectories(home);
      System.setProperty("idea.home.path", home.toString());
    }
    CoreApplicationEnvironment app = new CoreApplicationEnvironment(disposable, true, true);
    app.registerFileType(StarlarkFileType.INSTANCE, "bzl");
    app.registerParserDefinition(new StarlarkParserDefinition());
    CoreProjectEnvironment project = new CoreProjectEnvironment(disposable, app);
    return project.getProject();
  }

  private static void run(Project project, List<String> paths, Path root, Path out, boolean trim) throws IOException {
    for (String line : paths) {
      if (line.isBlank()) continue;
      String path = line.split("\t")[0];
      Path source = root.resolve(path);
      String text = StringUtil.convertLineSeparators(decode(Files.readAllBytes(source)));
      if (trim) text = trimText(text);
      LightVirtualFile virtualFile = new LightVirtualFile(source.getFileName().toString(), StarlarkLanguage.INSTANCE, text);
      virtualFile.setCharset(StandardCharsets.UTF_8);
      PsiFile file = ((PsiFileFactoryImpl) PsiFileFactory.getInstance(project))
        .trySetupPsiForFile(virtualFile, StarlarkLanguage.INSTANCE, false, false);
      if (file == null) throw new IllegalStateException("no Starlark PSI file for " + path);
      String dump = trimText(DebugUtil.psiToString(file, true, false));
      Path dumpPath = out.resolve(path + ".psi.txt");
      Files.createDirectories(dumpPath.getParent());
      Files.writeString(dumpPath, dump, StandardCharsets.UTF_8);
    }
  }

  private static String decode(byte[] bytes) throws IOException {
    return StandardCharsets.UTF_8.newDecoder()
      .onMalformedInput(CodingErrorAction.REPORT)
      .onUnmappableCharacter(CodingErrorAction.REPORT)
      .decode(ByteBuffer.wrap(bytes)).toString();
  }

  private static String trimText(String text) {
    int start = 0;
    int end = text.length();
    while (start < end && text.charAt(start) <= ' ') start++;
    while (end > start && text.charAt(end - 1) <= ' ') end--;
    return text.substring(start, end);
  }
}