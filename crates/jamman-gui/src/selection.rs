//! Selección múltiple de una lista, al estilo de los exploradores de ficheros:
//! clic selecciona uno, Ctrl+clic añade o quita y Mayús+clic selecciona un
//! rango desde el último clic. Independiente de la interfaz.

use std::collections::BTreeSet;

/// Teclas modificadoras relevantes de un clic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClickMods {
    /// Ctrl (Cmd en macOS): añadir o quitar.
    pub toggle: bool,
    /// Mayús: seleccionar un rango.
    pub range: bool,
}

/// Elementos seleccionados de una lista.
#[derive(Debug, Clone)]
pub struct Selection<K: Ord + Clone> {
    items: BTreeSet<K>,
    /// Último elemento pulsado: origen de los rangos y elemento "principal".
    anchor: Option<K>,
}

impl<K: Ord + Clone> Default for Selection<K> {
    fn default() -> Self {
        Self {
            items: BTreeSet::new(),
            anchor: None,
        }
    }
}

impl<K: Ord + Clone> Selection<K> {
    /// Aplica un clic sobre `key`. `order` es la lista tal como se ve, para
    /// calcular los rangos.
    pub fn click(&mut self, key: K, mods: ClickMods, order: &[K]) {
        if mods.range
            && let Some(anchor) = &self.anchor
            && let (Some(a), Some(b)) = (
                order.iter().position(|k| k == anchor),
                order.iter().position(|k| *k == key),
            )
        {
            if !mods.toggle {
                self.items.clear();
            }
            let (from, to) = if a <= b { (a, b) } else { (b, a) };
            self.items.extend(order[from..=to].iter().cloned());
            return;
        }
        if mods.toggle {
            if !self.items.remove(&key) {
                self.items.insert(key.clone());
            }
        } else {
            self.items.clear();
            self.items.insert(key.clone());
        }
        self.anchor = Some(key);
    }

    /// Selecciona solo `key`.
    pub fn set_single(&mut self, key: K) {
        self.items.clear();
        self.items.insert(key.clone());
        self.anchor = Some(key);
    }

    /// Selecciona todos los de `order`.
    pub fn select_all(&mut self, order: &[K]) {
        self.items = order.iter().cloned().collect();
    }

    /// Deja la selección vacía.
    pub fn clear(&mut self) {
        self.items.clear();
        self.anchor = None;
    }

    /// Quita los elementos que ya no existen.
    pub fn retain(&mut self, exists: impl Fn(&K) -> bool) {
        self.items.retain(|k| exists(k));
        if self.anchor.as_ref().is_some_and(|a| !exists(a)) {
            self.anchor = None;
        }
    }

    /// `true` si `key` está seleccionado.
    pub fn contains(&self, key: &K) -> bool {
        self.items.contains(key)
    }

    /// Elementos seleccionados, en orden.
    pub fn items(&self) -> impl Iterator<Item = &K> {
        self.items.iter()
    }

    /// Número de elementos seleccionados.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` si no hay nada seleccionado.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Elemento principal: el último pulsado si sigue seleccionado o, si no,
    /// el primero de la selección.
    pub fn primary(&self) -> Option<&K> {
        self.anchor
            .as_ref()
            .filter(|a| self.items.contains(*a))
            .or_else(|| self.items.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER: [u8; 5] = [1, 2, 3, 4, 5];
    const PLAIN: ClickMods = ClickMods {
        toggle: false,
        range: false,
    };
    const CTRL: ClickMods = ClickMods {
        toggle: true,
        range: false,
    };
    const SHIFT: ClickMods = ClickMods {
        toggle: false,
        range: true,
    };

    fn items(s: &Selection<u8>) -> Vec<u8> {
        s.items().copied().collect()
    }

    #[test]
    fn plain_click_selects_only_one() {
        let mut s = Selection::default();
        s.click(2, PLAIN, &ORDER);
        s.click(4, PLAIN, &ORDER);
        assert_eq!(items(&s), [4]);
        assert_eq!(s.primary(), Some(&4));
    }

    #[test]
    fn ctrl_click_adds_and_removes() {
        let mut s = Selection::default();
        s.click(1, PLAIN, &ORDER);
        s.click(3, CTRL, &ORDER);
        assert_eq!(items(&s), [1, 3]);
        s.click(1, CTRL, &ORDER);
        assert_eq!(items(&s), [3]);
        assert_eq!(
            s.primary(),
            Some(&3),
            "el principal pasa a uno que siga seleccionado"
        );
    }

    #[test]
    fn shift_click_selects_a_range_in_visible_order() {
        let mut s = Selection::default();
        s.click(4, PLAIN, &ORDER);
        s.click(2, SHIFT, &ORDER);
        assert_eq!(items(&s), [2, 3, 4]);
        // El orden visible manda, no el natural.
        let order = [5, 1, 3];
        s.click(5, PLAIN, &order);
        s.click(3, SHIFT, &order);
        assert_eq!(items(&s), [1, 3, 5]);
    }

    #[test]
    fn retain_drops_missing_items() {
        let mut s = Selection::default();
        s.select_all(&ORDER);
        s.click(5, CTRL, &ORDER); // quita el 5 y lo deja como último pulsado
        s.retain(|k| *k != 2);
        assert_eq!(items(&s), [1, 3, 4]);
        assert_eq!(s.primary(), Some(&1));
        s.clear();
        assert!(s.is_empty() && s.primary().is_none());
    }
}
