package net.ferrocraft.host;

import java.lang.reflect.Field;

final class GlCache {
    private final Field[] fields;
    private final Object[] targets;

    GlCache(Class<?> manager) {
        Object color = R.get(manager, "field_179170_t");
        Object blend = R.get(manager, "field_179157_e");
        Object enabled = R.get(blend, "field_179213_a");
        Object[] textures = (Object[]) R.get(manager, "field_179174_p");
        fields = new Field[10 + textures.length];
        targets = new Object[fields.length];
        add(0, color, "field_179195_a", "field_179193_b", "field_179194_c", "field_179192_d");
        add(4, blend, "field_179211_b", "field_179212_c", "field_179209_d", "field_179210_e");
        add(8, enabled, "field_179201_b");
        fields[9] = R.field(manager, "field_179162_o");
        Field textureName = R.field(textures[0], "field_179059_b");
        for (int i = 0; i < textures.length; i++) fields[10 + i] = textureName;
        System.arraycopy(textures, 0, targets, 10, textures.length);
    }

    private void add(int start, Object target, String... names) {
        for (int i = 0; i < names.length; i++) {
            fields[start + i] = R.field(target, names[i]);
            targets[start + i] = target;
        }
    }

    int[] save() {
        int[] values = new int[fields.length];
        try {
            for (int i = 0; i < fields.length; i++) {
                Field field = fields[i];
                Object target = targets[i];
                if (field.getType() == float.class) values[i] = Float.floatToRawIntBits(field.getFloat(target));
                else if (field.getType() == boolean.class) values[i] = field.getBoolean(target) ? 1 : 0;
                else values[i] = field.getInt(target);
            }
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
        return values;
    }

    void restore(int[] values) {
        try {
            for (int i = 0; i < fields.length; i++) {
                Field field = fields[i];
                Object target = targets[i];
                if (field.getType() == float.class) field.setFloat(target, Float.intBitsToFloat(values[i]));
                else if (field.getType() == boolean.class) field.setBoolean(target, values[i] != 0);
                else field.setInt(target, values[i]);
            }
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }
}
