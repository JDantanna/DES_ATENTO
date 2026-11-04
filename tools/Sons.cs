// Gerador dos sons do Navi, todos originais (nenhum áudio de jogo é usado).
// Imita o jeito de um console de 16 bits: instrumentos "amostrados" (sinos FM, harpa, marimba, cordas
// de sintetizador estilo anos 90 e metais), filtro suave que tira os agudos, uma pitada de ruído de
// quantização e o eco típico do chip de som. Gera WAV estéreo de 44,1 kHz, 16 bits.
//
// Uso (PowerShell): Add-Type -Path tools\Sons.cs; [Sons]::Gerar("D:\\pasta\\de\\saida")
using System;
using System.Collections.Generic;
using System.IO;

public static class Sons
{
    const int SR = 44100;
    static Random rnd = new Random(7);

    class Buf
    {
        public float[] L;
        public float[] R;
        public Buf(double secs)
        {
            int n = (int)(secs * SR);
            L = new float[n];
            R = new float[n];
        }
    }

    static double Hz(double midi) { return 440.0 * Math.Pow(2.0, (midi - 69.0) / 12.0); }

    static void Add(Buf b, double t0, double t, double v, double pan)
    {
        int i = (int)((t0 + t) * SR);
        if (i < 0 || i >= b.L.Length) return;
        double l = Math.Sqrt(0.5 * (1.0 - pan));
        double r = Math.Sqrt(0.5 * (1.0 + pan));
        b.L[i] += (float)(v * l * 1.41);
        b.R[i] += (float)(v * r * 1.41);
    }

    // Sino/celesta de modulação de fase (dois operadores, modulador inarmônico).
    static void Bell(Buf b, double t0, double midi, double dur, double vel, double pan)
    {
        double f = Hz(midi);
        double ring = dur + 0.9;
        int n = (int)(ring * SR);
        for (int k = 0; k < n; k++)
        {
            double t = (double)k / SR;
            double attack = Math.Min(1.0, t / 0.003);
            double env = attack * Math.Exp(-t * (2.2 + 380.0 / (f + 80.0)));
            double idx = 2.6 * Math.Exp(-t * 7.0);
            double mod = Math.Sin(2 * Math.PI * f * 3.5 * t);
            double y = Math.Sin(2 * Math.PI * f * t + idx * mod) + 0.25 * Math.Sin(2 * Math.PI * f * 2.0 * t) * Math.Exp(-t * 9.0);
            Add(b, t0, t, y * env * vel * 0.5, pan);
        }
    }

    // Harpa/pizzicato: harmônicos que decaem mais rápido quanto mais agudos.
    static void Pluck(Buf b, double t0, double midi, double dur, double vel, double pan)
    {
        double f = Hz(midi);
        double ring = dur + 0.6;
        int n = (int)(ring * SR);
        for (int k = 0; k < n; k++)
        {
            double t = (double)k / SR;
            double attack = Math.Min(1.0, t / 0.002);
            double y = 0;
            for (int h = 1; h <= 7; h++)
                y += Math.Sin(2 * Math.PI * f * h * t) / h * Math.Exp(-t * (2.5 + 2.2 * h));
            Add(b, t0, t, y * attack * vel * 0.6, pan);
        }
    }

    // Marimba: fundamental, quarto harmônico que some rápido e um estalo de ruído no ataque.
    static void Marimba(Buf b, double t0, double midi, double dur, double vel, double pan)
    {
        double f = Hz(midi);
        int n = (int)((dur + 0.4) * SR);
        for (int k = 0; k < n; k++)
        {
            double t = (double)k / SR;
            double y = Math.Sin(2 * Math.PI * f * t) * Math.Exp(-t * 9.0)
                     + 0.4 * Math.Sin(2 * Math.PI * f * 4.0 * t) * Math.Exp(-t * 32.0)
                     + (rnd.NextDouble() * 2 - 1) * 0.25 * Math.Exp(-t * 90.0);
            Add(b, t0, t, y * vel * 0.7, pan);
        }
    }

    // Cordas de sintetizador: serras desafinadas com ataque lento, agudos suavizados.
    static void Pad(Buf b, double t0, double midi, double dur, double vel, double pan)
    {
        double f = Hz(midi);
        double rel = 0.5;
        int n = (int)((dur + rel) * SR);
        for (int k = 0; k < n; k++)
        {
            double t = (double)k / SR;
            double env = Math.Min(1.0, t / 0.22);
            if (t > dur) env *= Math.Max(0.0, 1.0 - (t - dur) / rel);
            double y = 0;
            for (int h = 1; h <= 12; h++)
            {
                double amp = Math.Exp(-h / 5.0) / h;
                y += amp * (Math.Sin(2 * Math.PI * f * 0.997 * h * t) + Math.Sin(2 * Math.PI * f * 1.003 * h * t));
            }
            double vib = 1.0 + 0.0015 * Math.Sin(2 * Math.PI * 5.2 * t);
            y *= vib;
            Add(b, t0, t, y * env * vel * 0.32, pan);
        }
    }

    // Metal grave: serra com abertura de "filtro" que fecha e modulação de fase leve.
    static void Brass(Buf b, double t0, double midi, double dur, double vel, double pan)
    {
        double f = Hz(midi);
        int n = (int)((dur + 0.15) * SR);
        for (int k = 0; k < n; k++)
        {
            double t = (double)k / SR;
            double env = Math.Min(1.0, t / 0.03);
            if (t > dur) env *= Math.Max(0.0, 1.0 - (t - dur) / 0.15);
            double bright = 0.4 + 0.6 * Math.Exp(-t * 5.0);
            double y = 0;
            for (int h = 1; h <= 10; h++)
                y += Math.Sin(2 * Math.PI * f * h * t + 0.5 * Math.Sin(2 * Math.PI * f * t)) / h * Math.Pow(bright, h * 0.5);
            Add(b, t0, t, y * env * vel * 0.45, pan);
        }
    }

    // Suaviza os agudos, quantiza (textura de amostra antiga), aplica eco cruzado e normaliza.
    static short[] Master(Buf b)
    {
        int n = b.L.Length;
        float[][] ch = new float[][] { b.L, b.R };
        for (int c = 0; c < 2; c++)
        {
            float[] x = ch[c];
            float[] y = new float[n];
            for (int i = 1; i < n - 1; i++) y[i] = 0.22f * x[i - 1] + 0.56f * x[i] + 0.22f * x[i + 1];
            ch[c] = y;
        }
        // eco: atrasos diferentes em cada lado, realimentação cruzada
        int dl = (int)(0.17 * SR), dr = (int)(0.23 * SR);
        float[] l = ch[0], r = ch[1];
        for (int i = 0; i < n; i++)
        {
            if (i >= dl) l[i] += 0.34f * r[i - dl];
            if (i >= dr) r[i] += 0.34f * l[i - dr];
        }
        float peak = 1e-6f;
        for (int i = 0; i < n; i++) peak = Math.Max(peak, Math.Max(Math.Abs(l[i]), Math.Abs(r[i])));
        float g = 0.8f / peak;
        // corta o silêncio do final (o eco some antes de acabar o espaço reservado)
        int last = n - 1;
        while (last > 0 && Math.Max(Math.Abs(l[last]), Math.Abs(r[last])) < peak * 0.004f) last--;
        n = Math.Min(n, last + (int)(0.1 * SR));
        int fade = (int)(0.12 * SR);
        short[] pcm = new short[n * 2];
        for (int i = 0; i < n; i++)
        {
            float tail = i > n - fade ? (float)(n - i) / fade : 1f;
            double a = Math.Round(l[i] * g * tail * 2048.0) / 2048.0;
            double c = Math.Round(r[i] * g * tail * 2048.0) / 2048.0;
            pcm[2 * i] = (short)(a * 32767.0);
            pcm[2 * i + 1] = (short)(c * 32767.0);
        }
        return pcm;
    }

    static void Write(string path, short[] pcm)
    {
        using (BinaryWriter w = new BinaryWriter(File.Create(path)))
        {
            int bytes = pcm.Length * 2;
            w.Write(new char[] { 'R', 'I', 'F', 'F' });
            w.Write(36 + bytes);
            w.Write(new char[] { 'W', 'A', 'V', 'E', 'f', 'm', 't', ' ' });
            w.Write(16);
            w.Write((short)1);
            w.Write((short)2);
            w.Write(SR);
            w.Write(SR * 4);
            w.Write((short)4);
            w.Write((short)16);
            w.Write(new char[] { 'd', 'a', 't', 'a' });
            w.Write(bytes);
            for (int i = 0; i < pcm.Length; i++) w.Write(pcm[i]);
        }
    }

    delegate void Compose(Buf b);

    static void Make(string dir, string name, double secs, Compose c)
    {
        Buf b = new Buf(secs + 1.6);
        c(b);
        Write(Path.Combine(dir, name + ".wav"), Master(b));
    }

    public static void Gerar(string dir)
    {
        Directory.CreateDirectory(dir);

        // 01 - Abertura: arpejo de celesta subindo, sobre um acorde de cordas.
        Make(dir, "01", 2.2, delegate(Buf b)
        {
            Pad(b, 0.00, 60, 1.6, 0.8, -0.2); Pad(b, 0.00, 64, 1.6, 0.8, 0.2); Pad(b, 0.00, 67, 1.6, 0.8, 0.0);
            double[] n = new double[] { 72, 76, 79, 84, 88 };
            double[] t = new double[] { 0.00, 0.13, 0.26, 0.39, 0.56 };
            for (int i = 0; i < n.Length; i++) Bell(b, t[i], n[i], 0.35, 0.85, -0.5 + 0.25 * i);
            Bell(b, 0.80, 96, 0.5, 0.35, 0.3);
        });

        // 02 - Aviso suave: "ding-dong" de dois sinos.
        Make(dir, "02", 1.2, delegate(Buf b)
        {
            Bell(b, 0.00, 81, 0.5, 0.9, -0.2);
            Bell(b, 0.34, 76, 0.8, 0.9, 0.2);
        });

        // 03 - Aviso brilhante: três notas rápidas subindo e um brilho.
        Make(dir, "03", 1.0, delegate(Buf b)
        {
            Bell(b, 0.00, 88, 0.25, 0.8, -0.3);
            Bell(b, 0.09, 91, 0.25, 0.8, 0.0);
            Bell(b, 0.18, 95, 0.4, 0.85, 0.3);
            Bell(b, 0.32, 100, 0.5, 0.4, 0.0);
        });

        // 04 - Urgente: marimba repetida e um sino agudo.
        Make(dir, "04", 1.0, delegate(Buf b)
        {
            Marimba(b, 0.00, 84, 0.1, 0.9, -0.2);
            Marimba(b, 0.14, 84, 0.1, 0.9, 0.2);
            Marimba(b, 0.28, 84, 0.1, 0.9, -0.2);
            Bell(b, 0.44, 96, 0.5, 0.8, 0.1);
        });

        // 05 - Concluído: harpa subindo e brilho.
        Make(dir, "05", 1.2, delegate(Buf b)
        {
            double[] n = new double[] { 72, 76, 79, 84 };
            for (int i = 0; i < n.Length; i++) Pluck(b, 0.09 * i, n[i], 0.3, 0.9, -0.4 + 0.27 * i);
            Bell(b, 0.36, 96, 0.5, 0.7, 0.0);
            Bell(b, 0.50, 100, 0.5, 0.3, 0.2);
        });

        // 06 - Descartar: pizzicato descendo.
        Make(dir, "06", 0.9, delegate(Buf b)
        {
            Pluck(b, 0.00, 67, 0.25, 0.8, 0.2);
            Pluck(b, 0.16, 60, 0.5, 0.8, -0.2);
        });

        // 07 - Ir para a bandeja: cordas descendo, "recolhendo".
        Make(dir, "07", 1.0, delegate(Buf b)
        {
            Pad(b, 0.00, 76, 0.5, 0.9, -0.2);
            Pad(b, 0.25, 71, 0.7, 0.9, 0.2);
            Bell(b, 0.50, 83, 0.4, 0.35, 0.0);
        });

        // 08 - Voltar / abrir: cordas subindo e dois sinos.
        Make(dir, "08", 1.2, delegate(Buf b)
        {
            Pad(b, 0.00, 67, 0.5, 0.9, -0.2);
            Pad(b, 0.15, 72, 0.8, 0.9, 0.2);
            Bell(b, 0.35, 91, 0.4, 0.7, -0.2);
            Bell(b, 0.50, 96, 0.5, 0.7, 0.2);
        });

        // 09 - Erro / atenção: metal grave, dois toques.
        Make(dir, "09", 0.9, delegate(Buf b)
        {
            Brass(b, 0.00, 52, 0.18, 0.9, 0.0);
            Brass(b, 0.24, 52, 0.30, 0.9, 0.0);
            Marimba(b, 0.00, 40, 0.2, 0.5, 0.0);
        });

        // 10 - Conquista: acorde de cordas, arpejo de sinos e brilho de harpa.
        Make(dir, "10", 2.4, delegate(Buf b)
        {
            Pad(b, 0.00, 60, 1.9, 0.8, -0.3); Pad(b, 0.00, 64, 1.9, 0.8, 0.3); Pad(b, 0.00, 67, 1.9, 0.8, 0.0); Pad(b, 0.00, 72, 1.9, 0.7, 0.0);
            double[] n = new double[] { 72, 76, 79, 84, 88, 91 };
            double[] t = new double[] { 0.00, 0.15, 0.30, 0.45, 0.65, 0.65 };
            for (int i = 0; i < n.Length; i++) Bell(b, t[i], n[i], 0.5, 0.9, -0.5 + 0.2 * i);
            Pluck(b, 1.00, 96, 0.3, 0.5, 0.2);
            Pluck(b, 1.12, 100, 0.3, 0.4, -0.2);
        });

        // 11 - Atraso: dois sinos em tom menor, descendo.
        Make(dir, "11", 1.4, delegate(Buf b)
        {
            Pad(b, 0.00, 58, 0.9, 0.5, -0.2); Pad(b, 0.00, 65, 0.9, 0.5, 0.2);
            Bell(b, 0.00, 74, 0.4, 0.9, -0.2);
            Bell(b, 0.25, 70, 0.9, 0.9, 0.2);
        });

        // 12 - Despedida: sinos descendo devagar e cordas ao fundo.
        Make(dir, "12", 2.0, delegate(Buf b)
        {
            double[] n = new double[] { 91, 88, 84, 79, 76, 72 };
            for (int i = 0; i < n.Length; i++) Bell(b, 0.15 * i, n[i], 0.4, 0.8, 0.5 - 0.2 * i);
            Pad(b, 0.30, 60, 1.3, 0.6, -0.2); Pad(b, 0.30, 67, 1.3, 0.6, 0.2);
        });
    }
}
