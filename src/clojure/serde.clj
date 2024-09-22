(ns serde
  (:require [cheshire.core :as json])
  (:require [clojure.walk :as walk]))

(defn custom-serialize [data]
  (json/generate-string
    (walk/postwalk
      (fn [x]
        (if (keyword? x)
          (str x)
          x))
      data)))

(defn custom-deserialize [json-str]
  (let [data (json/parse-string json-str)]
    (walk/postwalk
      (fn [x]
        (if (string? x)
          (if (.startsWith x ":")
            (keyword (subs x 1))
            x)
          x))
      data)))