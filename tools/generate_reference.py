#!/usr/bin/env python3
"""Generate fixtures by compiling the original C# algorithms (requires .NET 10).

Usage: python3 tools/generate_reference.py /path/to/PavEcsGame
The checkout must be the pinned upstream commit. No source is downloaded.
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

PIN = 'bc0b449f68d953861932214b85110f99925d886a'
source = Path(sys.argv[1]).resolve()
root = Path(__file__).resolve().parents[1]
actual = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
if actual != PIN:
    raise SystemExit(f'Expected {PIN}, found {actual}')
common = source / 'PavEcsGame.Common'
components = source / 'PavEcsGame.Components'
lite = source / 'PavEcsLiteGame'
out = root / 'tests/fixtures'
out.mkdir(exist_ok=True)

with tempfile.TemporaryDirectory(prefix='pav-reference-') as directory:
    work = Path(directory)
    (work / 'Reference.csproj').write_text('''<Project Sdk="Microsoft.NET.Sdk">
<PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework>
<ImplicitUsings>enable</ImplicitUsings><Nullable>disable</Nullable></PropertyGroup>
</Project>''')
    for path in [common / 'Area/FieldOfViewComputation.cs',
                 common / 'Area/FieldOfViewComputationInt2.cs',
                 common / 'Utils/RangesCollectionV2.cs', common / 'Utils/Helper.cs',
                 common / 'Extensions/Int2Extensions.cs',
                 components / 'Components/PositionComponent.cs',
                 components / 'Components/LightSourceComponent.cs',
                 components / 'Components/LightValueComponent.cs']:
        shutil.copy(path, work / path.name)
    # Int2 is copied verbatim. Omit unrelated Float3, whose upstream operators
    # call Int2.Equals and prevent this standalone harness from compiling.
    math = (components / 'Types/MathTypes.cs').read_text(encoding='utf-8-sig')
    (work / 'Int2.cs').write_text(math.split('    public struct Float3')[0] + '\n}\n')
    light = (lite / 'Systems/Renders/LightRenderSystem.cs').read_text(encoding='utf-8-sig')
    # Extract the exact method and context, omitting the ECS-specific wrapper.
    begin = light.index('        private static void LightMerge(')
    end = light.rfind('\n    }')
    renderer = (lite / 'Systems/Renders/PrepareForRenderSystem.cs').read_text(encoding='utf-8-sig')
    colors = renderer[renderer.index('        private static readonly ConsoleColor[] _fireColors'):renderer.index('        public static System.ConsoleColor FromColor')]
    colors = colors.replace('public ConsoleColor ToConsoleColor', 'public static ConsoleColor ToConsoleColor')
    (work / 'ReferenceLight.cs').write_text('''using PavEcsGame;
using PavEcsGame.Components;
using PavEcsGame.Utils;
public static class ReferenceLight {
public static LightValueComponent Sample(byte value, LightType kind, byte baseValue,
    LightType sourceKind, int radius, int x, int y, float fov) {
    var result = new LightValueComponent { Value = value, LightType = kind };
    var source = new LightSourceComponent { Radius = radius,
        BasicParameters = new LightValueComponent { Value = baseValue, LightType = sourceKind } };
    var context = new LightDataContext(source, new PositionComponent(0, 0));
    LightMerge(context, new PositionComponent(x, y), ref result, fov);
    return result;
}
''' + light[begin:end] + '\n' + colors + '\n}\n')
    (work / 'Program.cs').write_text('''using System.Globalization;
using PavEcsGame.Area;
using PavEcsGame.Components;
CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
using var fovFile = new StreamWriter(Path.Combine(args[1], "fov_reference.tsv"));
fovFile.WriteLine("# PavEcsGame bc0b449; raw C# FOV samples, including floating-point roundoff");
void Emit(string name, Int2 origin, int radius, Func<Int2, Int2, bool> obstacle) {
    var computer = new FieldOfViewComputationInt2();
    (Int2 point, float value)[] samples = null;
    computer.Compute(origin, radius, obstacle, ref samples, out var count);
    fovFile.WriteLine($"case\\t{name}\\t{origin.X}\\t{origin.Y}\\t{radius}");
    for (int i = 0; i < count; i++) {
        var (p, v) = samples[i];
        fovFile.WriteLine($"{p.X}\\t{p.Y}\\t{v:R}");
    }
}
Emit("open", Int2.Zero, 4, (_, _) => false);
Emit("northwest", Int2.Zero, 4, (_, d) => d == new Int2(-1, -1));
Emit("east", Int2.Zero, 4, (_, d) => d == new Int2(1, 0));
Emit("ring", Int2.Zero, 4, (_, d) => Math.Max(Math.Abs(d.X), Math.Abs(d.Y)) == 1);
foreach (var name in new[] { "map1", "map2", "map3", "map1_test", "lightTest" }) {
    var lines = File.ReadAllLines(Path.Combine(args[0], name + ".txt"));
    int width = lines.Max(l => l.Length), height = lines.Length;
    Int2 origin = new Int2(width / 2, height / 2);
    var player = (from y in Enumerable.Range(0, height)
                  from x in Enumerable.Range(0, lines[y].Length)
                  where lines[y][x] == 'p' select new Int2(x, y)).ToArray();
    if (player.Length != 0) origin = player[0];
    Emit(name, origin, 16, (o, d) => {
        var p = o + d;
        if (p.X < 0 || p.Y < 0 || p.X >= width || p.Y >= height) return true;
        return p.X < lines[p.Y].Length && "Xx".Contains(lines[p.Y][p.X]);
    });
}
using var lightFile = new StreamWriter(Path.Combine(args[1], "light_reference.tsv"));
lightFile.WriteLine("# targetValue targetKind baseValue sourceKind radius x y fov resultValue resultKind");
foreach (byte kind in new byte[] {0, 1, 2, 4})
foreach (byte other in new byte[] {0, 1, 2, 4})
foreach (byte value in new byte[] {0, 1, 100, 200, 255})
foreach (int radius in new[] {3, 4, 16})
foreach (int x in new[] {0, 1, radius, radius + 1})
foreach (float fov in new[] {0f, 0.5f, 1f}) {
    var r = ReferenceLight.Sample(value, (LightType)kind, 196, (LightType)other, radius, x, 0, fov);
    lightFile.WriteLine($"{value}\\t{kind}\\t196\\t{other}\\t{radius}\\t{x}\\t0\\t{fov}\\t{r.Value}\\t{(byte)r.LightType}");
}
using var palette = new StreamWriter(Path.Combine(args[1], "palette_reference.tsv"));
palette.WriteLine("# kind value ConsoleColor");
foreach (byte kind in new byte[] {0, 1, 2, 4})
for (int value = 0; value <= 255; value++) {
    var color = ReferenceLight.ToConsoleColor(new LightValueComponent { Value = (byte)value, LightType = (LightType)kind });
    palette.WriteLine($"{kind}\\t{value}\\t{(byte)color}");
}
''')
    env = dict(os.environ, DOTNET_CLI_HOME=directory, DOTNET_NOLOGO='1', DOTNET_CLI_TELEMETRY_OPTOUT='1')
    subprocess.run(['dotnet', 'run', '--project', str(work / 'Reference.csproj'), '--',
                    str(common / 'Data'), str(out)], env=env, check=True)
print(f'Generated C# reference fixtures in {out}')
