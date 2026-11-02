namespace N
{
    class Type
    {
        } // Errant close curly

        private Constructor() { }
        ~Destructor() { }
        private static implicit operator int(Type t) => 0;
        event Action E1 { add { } remove { } }
        event Action E2, E3;
        private int field1, field2;
        private int this[int i] => 0;
        private void Method() { }
        public static Type operator+(Type t1, Type t2) => default;
        private int Prop => 0;
    }
}