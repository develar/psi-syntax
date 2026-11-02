
delegate void F(int x);
class C {
   void M() {
     F f = delegate (int x = 0) { };
   }
}
