delegate (int a, int b) I();
delegate (A.B, C.D) I();
delegate (A.B b, C.D d) I();
delegate (A.B<C>, T[]) I();
delegate (A.B<C> g, T[] a) I();
delegate ((A, B) c, D d) I();